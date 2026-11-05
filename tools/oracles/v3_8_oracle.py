#!/usr/bin/env python3
"""Independent oracle for SPEC_v3.8 (M17): filters, player selection and the four CSV reports,
computed from a poker_tracker SQLite database with nothing from the Rust code.

It reuses v3_7_oracle.py for the per-player-hand definitions of docs/stats.md (positions,
counters, money, all-in equity) and adds, independently of the crate:
  - the filter document of SPEC_v3.8 §5 and the definitions of docs/stats.md §9, evaluated in
    Python on the oracle's own values (never on hand_player_stats);
  - the player selection of SPEC_v3.8 §10 (the hero, or one screen name on one platform);
  - the results, positions, sessions and stats reports, byte for byte as docs/invariants.md §3
    specifies them, for any selection.

Usage (see tools/oracles/README.md; results and sessions need eval7, so use the venv):
    <venv>/bin/python -I tools/oracles/v3_8_oracle.py <db> --report <kind>
        [--filter <file.json>] [--player <platform>/<screen name>] [--stakes <sb>/<bb>[,...]]
    <venv>/bin/python -I tools/oracles/v3_8_oracle.py <db> --write-cases <golden dir>
        writes <case>/<kind>.csv for every <golden dir>/<case>/case.json
    <venv>/bin/python -I tools/oracles/v3_8_oracle.py <db> --check-cases <golden dir>
        compares instead of writing; exit status 1 on any difference

With no filter, no player and no stakes, the four reports of the v3.6 folder database must equal
tests/golden/v3_6_*.csv byte for byte; that is this script's self-check.
"""
import collections
import importlib.util
import json
import os
import sys
from datetime import date, timedelta
from fractions import Fraction

sys.dont_write_bytecode = True  # never leave __pycache__ in the repository

_spec = importlib.util.spec_from_file_location(
    "v3_7_oracle", os.path.join(os.path.dirname(os.path.abspath(__file__)), "v3_7_oracle.py"))
o = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(o)

KINDS = ("results", "positions", "sessions", "stats")
LABELS = ("EP", "MP", "CO", "BTN", "SB", "BB", "?")
RANKS, SUITS = "23456789TJQKA", "cdhs"
HEADERS = {
    "results": "currency,stakes,hands,net,net_per_100,bb_won,bb_per_100,rake,fees,blinds,"
               "straddles,allin_hands,ev_net,ev_bb_per_100",
    "positions": "currency,position,hands,net,bb_per_100,vpip,pfr,rfi",
    "sessions": "currency,platform,table,start,end,duration_seconds,hands,net,bb_per_100,ev_net",
    "stats": "currency,stat,numerator,denominator,value",
}
# Counter-filter columns (SPEC_v3.8 §5.3): every Flag and Count column of hand_player_stats.
FLAG_OR_COUNT = set(o.COUNTERS) | {"ev_qualified"}


class FilterError(Exception):
    pass


# ----------------------------------------------------------------------------------------------
# Selected player-hands
# ----------------------------------------------------------------------------------------------
class PlayerHand:
    """One player in one hand, with the oracle's own derived values."""

    def __init__(self, h, p, counters, pos):
        self.h, self.p = h, p
        self.c = counters
        self.pos = pos  # position code or None

    @property
    def label(self):
        return o.position_label(self.pos, self.h["max_seats"])

    def ev_qualified(self):
        ok, _, _, live = o.qualification(self.h)
        return int(ok and any(q["name"] == self.p["name"] for q in live))

    def value(self, column):
        if column == "ev_qualified":
            return self.ev_qualified()
        return self.c.get(column, 0)


def player_hands(hands, player):
    """The selected player's player-hands: the hero (player None) or (platform, screen name)."""
    out = []
    for h in hands:
        if player is None:
            ps = [p for p in h["players"] if p["hero"]]
        else:
            platform, name = player
            ps = [p for p in h["players"] if h["platform"] == platform and p["name"] == name]
        if not ps:
            continue
        cs, pos = o.counters(h)
        for p in ps:
            out.append(PlayerHand(h, p, cs[p["name"]], pos[p["seat"]]))
    return out


# ----------------------------------------------------------------------------------------------
# Filters (SPEC_v3.8 §5; definitions docs/stats.md §9)
# ----------------------------------------------------------------------------------------------
def parse_amount(s):
    """A plain decimal (digits, at most one '.', at most 6 decimals) as integer micro-units."""
    whole, _, frac = s.strip().partition(".")
    if not whole.isdigit() or (frac and not frac.isdigit()) or len(frac) > 6:
        raise FilterError(f"bad amount {s!r}")
    return int(whole) * 1_000_000 + int((frac or "0").ljust(6, "0"))


def parse_stakes(s):
    sb, sep, bb = s.partition("/")
    if not sep:
        raise FilterError(f"bad stakes {s!r}")
    return parse_amount(sb), parse_amount(bb)


def parse_date(s):
    if len(s) != 10 or s[4] != "-" or s[7] != "-":
        raise FilterError(f"bad date {s!r}")
    return date(int(s[0:4]), int(s[5:7]), int(s[8:10]))


def hole_class(cards):
    """'AKs', 'AKo' or '77' from two known cards; None otherwise."""
    if len(cards) != 2:
        return None
    (r1, s1), (r2, s2) = [(RANKS[c >> 2], SUITS[c & 3]) for c in cards]
    hi, lo = sorted((r1, r2), key=RANKS.index, reverse=True)
    if hi == lo:
        return hi + lo
    return hi + lo + ("s" if s1 == s2 else "o")


OPS = {"=": lambda a, b: a == b, "!=": lambda a, b: a != b, "<": lambda a, b: a < b,
       "<=": lambda a, b: a <= b, ">": lambda a, b: a > b, ">=": lambda a, b: a >= b}


def matches(node, ph):
    t = node["type"]
    h, p = ph.h, ph.p
    if t == "and":
        return all(matches(c, ph) for c in node["children"])
    if t == "or":
        return any(matches(c, ph) for c in node["children"])
    if t == "not":
        return not matches(node["child"], ph)
    if t == "currency":
        return h["currency"] in node["any_of"]
    if t == "stakes":
        return (h["sb"], h["bb"]) in {parse_stakes(s) for s in node["any_of"]}
    if t == "platform":
        return h["platform"] in node["any_of"]
    if t == "table_size":
        return node["min"] <= h["max_seats"] <= node["max"]
    if t == "table_name":
        return h["table"] in node["any_of"]
    if t == "date":
        d = h["ts"].date()   # the timestamp as printed (docs/stats.md §9)
        lo = parse_date(node["from"]) if "from" in node else date.min
        hi = parse_date(node["to"]) if "to" in node else date.max
        return lo <= d <= hi
    if t == "counter":
        if node["column"] not in FLAG_OR_COUNT:
            raise FilterError(f"not a counter: {node['column']}")
        return OPS[node["op"]](ph.value(node["column"]), node["value"])
    if t == "position":
        return ph.label in node["any_of"]
    if t == "result":
        net = p["net"]
        return {"net_won": net > 0, "net_lost": net < 0, "net_zero": net == 0}[node["is"]]
    if t == "pot_size":
        ok = True
        if "min_big_blinds" in node:
            ok = ok and h["pot"] >= node["min_big_blinds"] * h["bb"]
        if "max_big_blinds" in node:
            ok = ok and h["pot"] <= node["max_big_blinds"] * h["bb"]
        return ok
    if t == "went_all_in":
        return any(a["all_in"] and a["name"] == p["name"] for a in h["actions"])
    if t == "hole_cards":
        return hole_class(p["cards"]) in node["any_of"]
    raise FilterError(f"unknown node type {t!r}")


def load_filter(path):
    with open(path, encoding="utf-8") as f:
        doc = json.load(f)
    if doc.get("format_version") != 1:
        raise FilterError(f"{path}: format_version must be 1")
    return doc["filter"]


def select(hands, filt=None, player=None, stakes=None):
    """(selected player-hands, unmatched stakes items) for one report call."""
    base = player_hands(hands, player)
    unmatched = []
    if stakes is not None:
        wanted = []
        for s in stakes:
            if s not in wanted:
                wanted.append(s)
        for s in wanted:
            if not any((ph.h["sb"], ph.h["bb"]) == s for ph in base):
                unmatched.append(s)
        base = [ph for ph in base if (ph.h["sb"], ph.h["bb"]) in set(wanted)]
    if filt is not None:
        base = [ph for ph in base if matches(filt, ph)]
    return base, unmatched


# ----------------------------------------------------------------------------------------------
# Reports (docs/invariants.md §3, docs/stats.md §1.2, §5, §6)
# ----------------------------------------------------------------------------------------------
def q(text):
    """RFC 4180 quoting of a text field, only when needed."""
    if any(ch in text for ch in ',"\r\n'):
        return '"' + text.replace('"', '""') + '"'
    return text


def f2(x):
    """An exact Fraction rounded half-even to 2 decimals, printed with 2."""
    return o.fixed2(x.numerator, x.denominator)


def per100(total, hands):
    return f2(total * 100 / hands)


def bb_won(phs, key):
    return sum((Fraction(key(ph), ph.h["bb"]) for ph in phs), Fraction(0))


def ev_net_of(ph):
    return o.ev_net(ph.h, o.all_in_ev(ph.h))[ph.p["name"]]


def by_currency(phs):
    out = collections.defaultdict(list)
    for ph in phs:
        out[ph.h["currency"]].append(ph)
    return sorted(out.items())


def results_line(currency, stakes, phs):
    n = len(phs)
    net = sum(ph.p["net"] for ph in phs)
    money = collections.Counter()
    allin = 0
    for ph in phs:
        r = o.results(ph.h)[ph.p["name"]]
        for k in ("rake", "fees", "blinds", "straddles"):
            money[k] += r[k]
        allin += ph.ev_qualified()
    bbw = bb_won(phs, lambda ph: ph.p["net"])
    evn = sum(ev_net_of(ph) for ph in phs)
    evbb = bb_won(phs, ev_net_of)
    return (f"{q(currency)},{q(stakes)},{n},{o.money(net)},{per100(Fraction(net, 1_000_000), n)},"
            f"{f2(bbw)},{per100(bbw, n)},{o.money(money['rake'])},{o.money(money['fees'])},"
            f"{o.money(money['blinds'])},{o.money(money['straddles'])},{allin},{o.money(evn)},"
            f"{per100(evbb, n)}")


def report_results(phs):
    lines = []
    for currency, cphs in by_currency(phs):
        groups = collections.defaultdict(list)
        for ph in cphs:
            groups[(ph.h["bb"], ph.h["sb"])].append(ph)
        for (_, _), g in sorted(groups.items()):
            lines.append(results_line(currency, o.stakes(g[0].h), g))
        if len(groups) > 1:
            lines.append(results_line(currency, "ALL", cphs))
    return lines


def report_positions(phs):
    lines = []
    for currency, cphs in by_currency(phs):
        by_label = collections.defaultdict(list)
        for ph in cphs:
            by_label[ph.label].append(ph)
        for lab in LABELS:
            g = by_label.get(lab)
            if not g:
                continue
            tot = collections.Counter()
            for ph in g:
                tot.update({k: ph.value(k) for k in ("p_walk", "p_vpip", "p_pfr_opp", "p_pfr",
                                                      "p_rfi_opp", "p_rfi")})
            n = len(g)
            lines.append(
                f"{q(currency)},{q(lab)},{n},{o.money(sum(ph.p['net'] for ph in g))},"
                f"{per100(bb_won(g, lambda ph: ph.p['net']), n)},"
                f"{o.pct(tot['p_vpip'], n - tot['p_walk'])},{o.pct(tot['p_pfr'], tot['p_pfr_opp'])},"
                f"{o.pct(tot['p_rfi'], tot['p_rfi_opp'])}")
    return lines


def report_sessions(phs):
    by_table = collections.defaultdict(list)
    for ph in phs:
        by_table[(ph.h["platform"], ph.h["currency"], ph.h["table"])].append(ph)
    sessions = []
    for g in by_table.values():
        times = o.table_times([ph.h for ph in g])
        pairs = sorted(zip(times, g), key=lambda tp: (tp[0], tp[1].h["site_hand_id"]))
        cur = [pairs[0]]
        for tp in pairs[1:]:
            if int((tp[0] - cur[-1][0]).total_seconds()) > o.SESSION_GAP_SECONDS:
                sessions.append(cur)
                cur = [tp]
            else:
                cur.append(tp)
        sessions.append(cur)
    sessions.sort(key=lambda s: (s[0][1].h["currency"], s[0][1].h["ts"], s[0][1].h["table"],
                                 s[0][1].h["platform"]))
    lines = []
    for s in sessions:
        g = [ph for _, ph in s]
        first, last = g[0].h, g[-1].h
        n = len(g)
        lines.append(
            f"{q(first['currency'])},{q(first['platform'])},{q(first['table'])},"
            f"{first['ts'].isoformat()},{last['ts'].isoformat()},{o.duration_seconds([t for t, _ in s])},"
            f"{n},{o.money(sum(ph.p['net'] for ph in g))},{per100(bb_won(g, lambda ph: ph.p['net']), n)},"
            f"{o.money(sum(ev_net_of(ph) for ph in g))}")
    return lines


def report_stats(phs):
    lines = []
    for currency, cphs in by_currency(phs):
        tot = collections.Counter()
        for ph in cphs:
            tot.update(ph.c)
        tot["hands_ex_walks"] = tot["hands"] - tot["p_walk"]
        lines += o.stats_rows(tot, q(currency))
    return lines


REPORTS = {"results": report_results, "positions": report_positions,
           "sessions": report_sessions, "stats": report_stats}


def report_csv(kind, phs):
    return "".join(line + "\n" for line in [HEADERS[kind]] + REPORTS[kind](phs))


# ----------------------------------------------------------------------------------------------
# Command line
# ----------------------------------------------------------------------------------------------
def parse_player(s):
    platform, sep, name = s.partition("/")
    if not sep or not name:
        raise FilterError(f"bad player {s!r}: expected <platform>/<screen name>")
    return platform, name


def run_case(hands, case_dir, root):
    """The four CSVs of one golden case: {kind: text}, and the unmatched stakes."""
    with open(os.path.join(case_dir, "case.json"), encoding="utf-8") as f:
        case = json.load(f)
    filt = load_filter(os.path.join(root, case["filter"])) if case.get("filter") else None
    player = parse_player(case["player"]) if case.get("player") else None
    stakes = [parse_stakes(s) for s in case["stakes"]] if case.get("stakes") else None
    phs, unmatched = select(hands, filt, player, stakes)
    return {k: report_csv(k, phs) for k in KINDS}, unmatched, case


def main():
    args = sys.argv[1:]
    if not args or args[0].startswith("-"):
        sys.exit(__doc__)
    hands = o.load(args[0])
    root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    if len(args) == 3 and args[1] in ("--write-cases", "--check-cases"):
        golden, ok = args[2], True
        for name in sorted(os.listdir(golden)):
            case_dir = os.path.join(golden, name)
            if not os.path.isfile(os.path.join(case_dir, "case.json")):
                continue
            out, unmatched, case = run_case(hands, case_dir, root)
            want = [o.money(sb) + "/" + o.money(bb) for sb, bb in unmatched]
            if want != case.get("unmatched_stakes", []):
                print(f"{name}: unmatched stakes {want}, case.json says {case.get('unmatched_stakes', [])}")
                ok = False
            rows = out["results"].count("\n") - 1
            hand_count = sum(int(line.split(",")[2]) for line in out["results"].splitlines()[1:]
                             if line.split(",")[1] != "ALL")
            for kind, text in out.items():
                path = os.path.join(case_dir, f"{kind}.csv")
                if args[1] == "--write-cases":
                    with open(path, "w", encoding="utf-8", newline="\n") as f:
                        f.write(text)
                else:
                    with open(path, encoding="utf-8", newline="") as f:
                        if f.read() != text:
                            print(f"{name}/{kind}.csv: DIFFERENT")
                            ok = False
            print(f"{name}: {hand_count} player-hands, {rows} results row(s), "
                  f"{out['sessions'].count(chr(10)) - 1} session(s)")
        sys.exit(0 if ok else 1)
    kind = args[args.index("--report") + 1]
    filt = load_filter(args[args.index("--filter") + 1]) if "--filter" in args else None
    player = parse_player(args[args.index("--player") + 1]) if "--player" in args else None
    stakes = ([parse_stakes(s) for s in args[args.index("--stakes") + 1].split(",")]
              if "--stakes" in args else None)
    phs, unmatched = select(hands, filt, player, stakes)
    for sb, bb in unmatched:
        print(f"No hands at stakes {o.money(sb)}/{o.money(bb)}", file=sys.stderr)
    sys.stdout.write(report_csv(kind, phs))


if __name__ == "__main__":
    main()
