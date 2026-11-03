#!/usr/bin/env python3
"""Independent oracle for SPEC_v3.7 (M13-M16): positions, stat counters, results, sessions and
all-in equity, computed from a poker_tracker SQLite database with nothing from the Rust code.

Usage (see tools/oracles/README.md):
    python3 -I tools/oracles/v3_7_oracle.py --write-fixture EV_CASHOUT_RECEIVED /tmp/oracle_extra/hand.txt
    cargo run --release --locked -- tests/fixtures/ggpoker --ggpoker-currency CAD --db /tmp/o.sqlite
    cargo run --release --locked -- /tmp/oracle_extra --ggpoker-currency CAD --db /tmp/o.sqlite
    python3 -I tools/oracles/v3_7_oracle.py /tmp/o.sqlite            # M13-M15
    <venv>/bin/python -I tools/oracles/v3_7_oracle.py /tmp/o.sqlite --equity   # + M16 (eval7)

Output sections are labelled with the SPEC_v3.7 section that pins them.

The definitions implemented here are the ones in docs/stats.md; if the two disagree, stats.md
is right and this script has a bug.
"""
import collections
import itertools
import math
import os
import sqlite3
import sys
from datetime import datetime

STREETS = ("p", "f", "t", "r")
POSTS = {"PostSmallBlind", "PostBigBlind", "PostDeadBlind", "PostStraddle"}
CONTRIB = POSTS | {"Call", "Bet", "Raise"}
SESSION_GAP_SECONDS = 30 * 60


# ----------------------------------------------------------------------------------------------
# Loading (integer micro-units throughout)
# ----------------------------------------------------------------------------------------------
def load(db):
    c = sqlite3.connect(db)
    c.row_factory = sqlite3.Row
    names = {r["id"]: r["screen_name"] for r in c.execute("SELECT id, screen_name FROM players")}
    hands = {}
    for r in c.execute("SELECT * FROM hands ORDER BY id"):
        hands[r["id"]] = dict(
            id=r["id"], site_hand_id=r["site_hand_id"], platform=r["platform"],
            currency=r["currency"], sb=r["small_blind_micros"], bb=r["big_blind_micros"],
            ts=datetime.fromisoformat(r["timestamp"]), table=r["table_name"],
            max_seats=r["max_seats"], button=r["button_seat"], pot=r["total_pot_micros"],
            file=os.path.basename(r["source_file"] or ""), players=[], actions=[], boards=[],
            winners=[], deductions=[], adjustments=[])
    for r in c.execute("SELECT * FROM hand_players ORDER BY hand_id, seat"):
        hands[r["hand_id"]]["players"].append(dict(
            seat=r["seat"], name=names[r["player_id"]], stack=r["starting_stack_micros"],
            cards=[x for x in (r["hole_card_1"], r["hole_card_2"]) if x is not None],
            hero=bool(r["is_hero"]), invested=r["invested_micros"], won=r["won_micros"],
            adj=r["adjustments_micros"], net=r["net_micros"]))
    for r in c.execute("SELECT * FROM actions ORDER BY hand_id, sequence"):
        hands[r["hand_id"]]["actions"].append(dict(
            street=STREETS[r["street"]], name=names[r["player_id"]], type=r["action_type"],
            amount=r["amount_micros"], total=r["street_total_after_micros"],
            all_in=bool(r["is_all_in"])))
    for r in c.execute("SELECT * FROM board_cards ORDER BY hand_id, run_index"):
        hands[r["hand_id"]]["boards"].append(
            [x for x in (r["flop_1"], r["flop_2"], r["flop_3"], r["turn"], r["river"]) if x is not None])
    for r in c.execute("SELECT * FROM pot_winners ORDER BY hand_id, pot_sequence"):
        hands[r["hand_id"]]["winners"].append((names[r["player_id"]], r["amount_micros"]))
    for r in c.execute("SELECT * FROM pot_deductions ORDER BY hand_id, deduction_sequence"):
        hands[r["hand_id"]]["deductions"].append((r["kind"], r["amount_micros"]))
    for r in c.execute("SELECT * FROM player_adjustments ORDER BY hand_id, adjustment_sequence"):
        hands[r["hand_id"]]["adjustments"].append(
            (names[r["player_id"]], r["kind"], r["amount_micros"], STREETS[r["street"]]))
    return list(hands.values())


# ----------------------------------------------------------------------------------------------
# M13: positions (docs/stats.md §2)
# ----------------------------------------------------------------------------------------------
def clockwise_after(seats, start):
    s = sorted(seats)
    return [x for x in s if x > start] + [x for x in s if x <= start]


def positions(h):
    """seat -> position code (0 BTN, 1 CO, 2.., 8 BB, 9 SB) or None for the whole hand."""
    seats = [p["seat"] for p in h["players"]]
    seat_of = {p["name"]: p["seat"] for p in h["players"]}
    sbs = [a for a in h["actions"] if a["type"] == "PostSmallBlind"]
    bbs = [a for a in h["actions"] if a["type"] == "PostBigBlind"]
    btn = h["button"]
    if btn not in seats or len(sbs) != 1 or not bbs:
        return {s: None for s in seats}
    sb, bb = seat_of[sbs[0]["name"]], seat_of[bbs[0]["name"]]
    if len(seats) == 2:
        ok = sb == btn and bb != btn
    else:
        ok = clockwise_after(seats, btn)[0] == sb and clockwise_after(seats, sb)[0] == bb
    if not ok:
        return {s: None for s in seats}
    pos = {sb: 9, bb: 8}
    code = 0
    # counter-clockwise from the button: BTN 0, then the seat to its right 1 (CO), ...
    ccw = [btn] + list(reversed(clockwise_after(seats, btn)))[:-1]
    for s in ccw:
        if s in pos:
            continue
        pos[s] = code
        code += 1
    return pos


def position_label(code, max_seats):
    if code is None:
        return "?"
    if code == 9:
        return "SB"
    if code == 8:
        return "BB"
    if code == 0:
        return "BTN"
    if code == 1:
        return "CO"
    if max_seats <= 6:
        return "MP" if code == 2 else "EP"
    return "MP" if code <= 4 else "EP"


# ----------------------------------------------------------------------------------------------
# M13 + M14: per player-hand counters (docs/stats.md §3-§4)
# ----------------------------------------------------------------------------------------------
COUNTERS = [
    # M13 facts
    "hands", "p_vpip", "p_pfr", "f_saw", "t_saw", "r_saw", "sd_went", "sd_won", "won_hand", "f_saw_won",
    # M14 preflop
    "p_rfi_opp", "p_rfi", "p_3bet_opp", "p_3bet", "p_3bet_faced", "p_3bet_fold", "p_3bet_call",
    "p_4bet_opp", "p_4bet", "p_4bet_faced", "p_4bet_fold", "p_4bet_call", "p_cc_opp", "p_cc",
    "p_steal_opp", "p_steal", "p_sb_steal_faced", "p_sb_steal_fold", "p_sb_steal_raise",
    "p_bb_steal_faced", "p_bb_steal_fold", "p_bb_steal_raise",
    # M14 postflop
    "f_cbet_opp", "f_cbet", "f_cbet_faced", "f_cbet_fold", "f_cbet_call", "f_cbet_raise",
    "t_cbet_opp", "t_cbet", "t_cbet_faced", "t_cbet_fold", "t_cbet_call", "t_cbet_raise",
    "f_xr_opp", "f_xr",
] + [f"{s}_{k}" for s in STREETS for k in ("bets", "raises", "calls", "checks", "folds")]


class Street:
    """Replays one street, exposing the state each voluntary decision was made in."""

    def __init__(self, h, street, commit_start, gross, folded, all_in):
        self.h, self.street = h, street
        self.committed = dict(commit_start)
        self.gross, self.folded, self.all_in = gross, folded, all_in
        self.stack = {p["name"]: p["stack"] for p in h["players"]}

    def decisions(self):
        raises = calls = bets = 0
        for i, a in enumerate(self.h["actions"]):
            if a["street"] != self.street or a["type"] in POSTS or a["type"] == "UncalledBetReturned":
                if a["street"] == self.street and a["type"] == "UncalledBetReturned":
                    self.committed[a["name"]] = a["total"]
                continue
            n = a["name"]
            facing = max(self.committed.values(), default=0)
            to_call = facing - self.committed.get(n, 0)
            remaining = self.stack[n] - self.gross[n]
            others_can_act = any(q != n and q not in self.folded and q not in self.all_in
                                 for q in self.stack)
            state = dict(index=i, action=a, raises=raises, calls=calls, bets=bets, to_call=to_call,
                         can_raise=remaining > to_call and others_can_act,
                         can_bet=to_call == 0 and remaining > 0 and others_can_act)
            yield state
            t = a["type"]
            if t in CONTRIB:
                self.gross[n] += a["amount"]
                if self.gross[n] == self.stack[n]:
                    self.all_in.add(n)
            self.committed[n] = a["total"]
            if t == "Fold":
                self.folded.add(n)
            elif t == "Raise":
                raises += 1
            elif t == "Call":
                calls += 1
            elif t == "Bet":
                bets += 1


def counters(h):
    pos = positions(h)
    seat_of = {p["name"]: p["seat"] for p in h["players"]}
    out = {p["name"]: collections.Counter(hands=1) for p in h["players"]}
    gross = {p["name"]: 0 for p in h["players"]}
    folded, all_in = set(), set()
    posted = set()
    commit = {}
    for a in h["actions"]:
        if a["type"] in POSTS:
            posted.add(a["name"])
            gross[a["name"]] += a["amount"]
            if gross[a["name"]] == next(p["stack"] for p in h["players"] if p["name"] == a["name"]):
                all_in.add(a["name"])
            commit[a["name"]] = a["total"]
    straddled = any(a["type"] == "PostStraddle" for a in h["actions"])

    dealt = {"f": any(len(b) >= 3 for b in h["boards"]), "t": any(len(b) >= 4 for b in h["boards"]),
             "r": any(len(b) >= 5 for b in h["boards"])}

    # ---------------- preflop ----------------
    first = {}           # name -> first decision state
    first_with = {}      # (name, key) -> first decision state matching key
    last_raiser = None
    steal_raiser = None  # name of a steal attempt (first raise of the hand, if it was a steal)
    pre = Street(h, "p", commit, gross, folded, all_in)
    first_raise_seen = False
    for st in pre.decisions():
        a = st["action"]; n = a["name"]; t = a["type"]; c = out[n]
        c[f"p_{t.lower()}s" if t != "Check" else "p_checks"] += 1
        if t in ("Call", "Raise"):
            c["p_vpip"] = 1
        if t == "Raise":
            c["p_pfr"] = 1
        if n not in first:
            first[n] = st
            unopened = st["raises"] == 0 and st["calls"] == 0
            if unopened and st["can_raise"]:
                c["p_rfi_opp"] = 1
                c["p_rfi"] = int(t == "Raise")
                if not straddled and pos[seat_of[n]] in (0, 1, 9):
                    c["p_steal_opp"] = 1
                    c["p_steal"] = int(t == "Raise")
            if st["raises"] >= 1 and n not in posted:
                c["p_cc_opp"] = 1
                c["p_cc"] = int(t == "Call")
        if st["raises"] == 1 and (n, "3bet_opp") not in first_with:
            first_with[(n, "3bet_opp")] = st
            if st["can_raise"]:
                c["p_3bet_opp"] = 1
                c["p_3bet"] = int(t == "Raise")
        if st["raises"] == 2 and (n, "3bet_faced") not in first_with:
            first_with[(n, "3bet_faced")] = st
            c["p_3bet_faced"] = 1
            c["p_3bet_fold"] = int(t == "Fold")
            c["p_3bet_call"] = int(t == "Call")
        if st["raises"] >= 2 and (n, "4bet_opp") not in first_with:
            first_with[(n, "4bet_opp")] = st
            if st["can_raise"]:
                c["p_4bet_opp"] = 1
                c["p_4bet"] = int(t == "Raise")
        if st["raises"] >= 3:
            if (n, "4bet_faced") not in first_with:
                first_with[(n, "4bet_faced")] = st
                c["p_4bet_faced"] = 1
                c["p_4bet_call"] = int(t == "Call")
            if t == "Fold":
                c["p_4bet_fold"] = 1
        # steal defence: blind's first decision after a steal with only folds in between
        if steal_raiser and st["raises"] == 1 and st["calls"] == 0 and n != steal_raiser:
            code = pos[seat_of[n]]
            key = "sb" if code == 9 else "bb" if code == 8 else None
            if key and (n, "steal_def") not in first_with:
                first_with[(n, "steal_def")] = st
                c[f"p_{key}_steal_faced"] = 1
                c[f"p_{key}_steal_fold"] = int(t == "Fold")
                c[f"p_{key}_steal_raise"] = int(t == "Raise")
        if t == "Raise":
            # the first raise of the hand is a steal attempt iff it was made at a steal opportunity;
            # a later raise or call ends "facing a steal" through raises == 1 and calls == 0 above
            if not first_raise_seen and c["p_steal"] == 1:
                steal_raiser = n
            first_raise_seen = True
            last_raiser = n

    # ---------------- postflop ----------------
    aggressor = last_raiser      # flop c-bet candidate
    for s in ("f", "t", "r"):
        if not dealt[s]:
            break
        for p in h["players"]:
            if p["name"] not in folded:
                out[p["name"]][f"{s}_saw"] = 1
        street = Street(h, s, {}, gross, folded, all_in)
        cbet_by = None
        raise_after_cbet = False
        first_dec = {}
        checked = set()
        next_aggressor = None
        for st in street.decisions():
            a = st["action"]; n = a["name"]; t = a["type"]; c = out[n]
            c[f"{s}_{'checks' if t == 'Check' else t.lower() + 's'}"] += 1
            is_first = n not in first_dec
            if is_first:
                first_dec[n] = st
            if s in ("f", "t") and n == aggressor and is_first and st["bets"] == 0 and st["can_bet"]:
                c[f"{s}_cbet_opp"] = 1
                c[f"{s}_cbet"] = int(t == "Bet")
                if t == "Bet":
                    cbet_by = n
                    next_aggressor = n
                    continue
            if s in ("f", "t") and cbet_by and n != cbet_by and not raise_after_cbet \
                    and (n, s, "cbet_faced") not in first_with:
                first_with[(n, s, "cbet_faced")] = st
                c[f"{s}_cbet_faced"] = 1
                c[f"{s}_cbet_fold"] = int(t == "Fold")
                c[f"{s}_cbet_call"] = int(t == "Call")
                c[f"{s}_cbet_raise"] = int(t == "Raise")
            if cbet_by and t == "Raise":
                raise_after_cbet = True
            if s == "f":
                if is_first and t == "Check":
                    checked.add(n)
                elif n in checked and st["to_call"] > 0 and (n, "xr") not in first_with:
                    first_with[(n, "xr")] = st
                    c["f_xr_opp"] = 1
                    c["f_xr"] = int(t == "Raise")
        aggressor = next_aggressor   # turn c-bet only after a flop c-bet

    live = [p for p in h["players"] if p["name"] not in folded]
    for p in h["players"]:
        c = out[p["name"]]
        if p["won"] > 0:
            c["won_hand"] = 1
            if c["f_saw"]:
                c["f_saw_won"] = 1
        if len(live) >= 2 and p["name"] not in folded:
            c["sd_went"] = 1
            c["sd_won"] = int(p["won"] > 0)
    return out, pos


# ----------------------------------------------------------------------------------------------
# M15: results (docs/stats.md §6.1)
# ----------------------------------------------------------------------------------------------
def largest_remainder(total, weights, seats):
    """Split integer `total` in proportion to `weights`; leftovers to the largest remainders,
    ties to the lower seat."""
    w = sum(weights)
    if w == 0 or total == 0:
        return [0] * len(weights)
    base = [total * x // w for x in weights]
    rem = [total * x % w for x in weights]
    left = total - sum(base)
    order = sorted(range(len(weights)), key=lambda i: (-rem[i], seats[i]))
    for i in order[:left]:
        base[i] += 1
    return base


def results(h):
    ps = h["players"]
    seats = [p["seat"] for p in ps]
    inv = [p["invested"] for p in ps]
    rake = sum(a for k, a in h["deductions"] if k == "Rake")
    fees = sum(a for k, a in h["deductions"] if k != "Rake")
    r = largest_remainder(rake, inv, seats)
    f = largest_remainder(fees, inv, seats)
    blinds = collections.Counter(); straddles = collections.Counter()
    for a in h["actions"]:
        if a["type"] in ("PostSmallBlind", "PostBigBlind", "PostDeadBlind"):
            blinds[a["name"]] += a["amount"]
        elif a["type"] == "PostStraddle":
            straddles[a["name"]] += a["amount"]
    return {p["name"]: dict(net=p["net"], rake=r[i], fees=f[i], blinds=blinds[p["name"]],
                            straddles=straddles[p["name"]]) for i, p in enumerate(ps)}


def sessions(hands):
    """Hero table sessions: same (platform, currency, table), consecutive hands (ordered by
    timestamp, then site hand id) at most 30 min apart."""
    out = []
    by_table = collections.defaultdict(list)
    for h in hands:
        if any(p["hero"] for p in h["players"]):
            by_table[(h["platform"], h["currency"], h["table"])].append(h)
    for key, hs in by_table.items():
        hs.sort(key=lambda h: (h["ts"], h["site_hand_id"]))
        cur = [hs[0]]
        for h in hs[1:]:
            if (h["ts"] - cur[-1]["ts"]).total_seconds() > SESSION_GAP_SECONDS:
                out.append(cur); cur = [h]
            else:
                cur.append(h)
        out.append(cur)
    out.sort(key=lambda s: (s[0]["ts"], s[0]["table"]))
    return out


# ----------------------------------------------------------------------------------------------
# M16: all-in equity (docs/stats.md §6.2)
# ----------------------------------------------------------------------------------------------
def exact_equity(holes, board):
    """Exact equity of each hole-card pair over every completion of `board`, as reduced
    (num, den) pairs. Only the given hole cards and the board are removed from the deck (dead
    cards are not: docs/stats.md §6.2). Needs eval7."""
    import eval7
    ranks, suits = "23456789TJQKA", "cdhs"
    card = lambda c: eval7.Card(ranks[c >> 2] + suits[c & 3])
    used = set(board) | {c for h in holes for c in h}
    deck = [c for c in range(52) if c not in used]
    lcm = math.lcm(*range(1, len(holes) + 1))
    acc = [0] * len(holes)
    n = 0
    hb = [[card(c) for c in h] for h in holes]
    bb = [card(c) for c in board]
    for extra in itertools.combinations(deck, 5 - len(board)):
        b = bb + [card(c) for c in extra]
        v = [eval7.evaluate(x + b) for x in hb]
        top = max(v)
        win = [i for i, x in enumerate(v) if x == top]
        for i in win:
            acc[i] += lcm // len(win)
        n += 1
    out = []
    for a in acc:
        g = math.gcd(a, n * lcm)
        out.append((a // g, n * lcm // g))
    return out


def round_half_even_div(num, den):
    q, r = divmod(num, den)
    if 2 * r > den or (2 * r == den and q % 2 == 1):
        q += 1
    return q


def qualification(h):
    """(qualifies, reason, all-in street, live players)."""
    folded = {a["name"] for a in h["actions"] if a["type"] == "Fold"}
    live = [p for p in h["players"] if p["name"] not in folded]
    if len(live) < 2:
        return False, "uncontested", None, live
    last = [a for a in h["actions"] if a["type"] != "UncalledBetReturned"][-1]
    if last["street"] == "r":
        return False, "betting reached the river", None, live
    if any(len(p["cards"]) != 2 for p in live):
        return False, "hole cards unknown", None, live
    dead = collections.Counter()
    for a in h["actions"]:
        if a["type"] == "PostDeadBlind":
            dead[a["name"]] += a["amount"]
    if len({p["invested"] - dead[p["name"]] for p in live}) != 1:
        return False, "side pots", None, live
    return True, None, last["street"], live


def board_at(h, street):
    n = {"p": 0, "f": 3, "t": 4}[street]
    return h["boards"][0][:n] if h["boards"] else []


_EV_CACHE = {}


def all_in_ev(h):
    """None if the hand does not qualify, else the all-in street, pot after deductions and
    {name: (num, den, share_micros)} for the live players (memoized per stored hand)."""
    if h["id"] not in _EV_CACHE:
        _EV_CACHE[h["id"]] = _all_in_ev(h)
    return _EV_CACHE[h["id"]]


def _all_in_ev(h):
    ok, _, street, live = qualification(h)
    if not ok:
        return None
    eqs = exact_equity([p["cards"] for p in live], board_at(h, street))
    pot_after = h["pot"] - sum(a for _, a in h["deductions"])
    res = {p["name"]: (num, den, round_half_even_div(num * pot_after, den))
           for p, (num, den) in zip(live, eqs)}
    return dict(street=street, pot_after=pot_after, players=res)


def ev_net(h, ev):
    """{name: ev_net_micros} for every player of the hand."""
    out = {p["name"]: p["net"] for p in h["players"]}
    if ev is None:
        return out
    cashed = {n for n, k, _, _ in h["adjustments"] if k.startswith("EvCashout")}
    for p in h["players"]:
        if p["name"] in ev["players"] and p["name"] not in cashed:
            out[p["name"]] = ev["players"][p["name"]][2] - p["invested"] + p["adj"]
    return out


# ----------------------------------------------------------------------------------------------
# Output
# ----------------------------------------------------------------------------------------------
def money(m):
    """Exact micro-units as a decimal string, trailing zeros removed."""
    s = f"{abs(m) // 1_000_000}.{abs(m) % 1_000_000:06d}".rstrip("0").rstrip(".")
    return ("-" if m < 0 else "") + s


def fixed2(num, den):
    """num/den rounded half-even to 2 decimals, always printed with 2 decimals."""
    q = round_half_even_div(num * 100, den)
    sign = "-" if q < 0 else ""
    q = abs(q)
    return f"{sign}{q // 100}.{q % 100:02d}"


def pct(num, den):
    return "" if den == 0 else fixed2(num * 100, den)


def hero(h):
    return next(p for p in h["players"] if p["hero"])


def stakes(h):
    return f"{money(h['sb'])}/{money(h['bb'])}"


STAT_ROWS = [  # (report id, numerator, denominator) in docs/stats.md §5 order
    ("vpip", "p_vpip", "hands"), ("pfr", "p_pfr", "hands"), ("saw_flop", "f_saw", "hands"),
    ("wtsd", "sd_went", "f_saw"), ("wsd", "sd_won", "sd_went"), ("wwsf", "f_saw_won", "f_saw"),
    ("win", "won_hand", "hands"), ("rfi", "p_rfi", "p_rfi_opp"), ("three_bet_pf", "p_3bet", "p_3bet_opp"),
    ("fold_to_three_bet_pf", "p_3bet_fold", "p_3bet_faced"), ("call_three_bet_pf", "p_3bet_call", "p_3bet_faced"),
    ("four_bet_pf", "p_4bet", "p_4bet_opp"), ("fold_to_four_bet_pf", "p_4bet_fold", "p_4bet_faced"),
    ("call_four_bet_pf", "p_4bet_call", "p_4bet_faced"), ("cold_call_pf", "p_cc", "p_cc_opp"),
    ("att_to_steal", "p_steal", "p_steal_opp"), ("fold_sb_to_steal", "p_sb_steal_fold", "p_sb_steal_faced"),
    ("raise_sb_steal", "p_sb_steal_raise", "p_sb_steal_faced"), ("fold_bb_to_steal", "p_bb_steal_fold", "p_bb_steal_faced"),
    ("raise_bb_steal", "p_bb_steal_raise", "p_bb_steal_faced"), ("cbet_flop", "f_cbet", "f_cbet_opp"),
    ("fold_to_cbet_flop", "f_cbet_fold", "f_cbet_faced"), ("call_cbet_flop", "f_cbet_call", "f_cbet_faced"),
    ("raise_cbet_flop", "f_cbet_raise", "f_cbet_faced"), ("cbet_turn", "t_cbet", "t_cbet_opp"),
    ("fold_to_cbet_turn", "t_cbet_fold", "t_cbet_faced"), ("call_cbet_turn", "t_cbet_call", "t_cbet_faced"),
    ("raise_cbet_turn", "t_cbet_raise", "t_cbet_faced"), ("check_raise_flop", "f_xr", "f_xr_opp"),
]
STREET_NAMES = {"p": "pf", "f": "flop", "t": "turn", "r": "river"}


def stats_rows(tot, currency):
    rows = [f"{currency},{n},{tot[a]},{tot[b]},{pct(tot[a], tot[b])}" for n, a, b in STAT_ROWS]
    agg = lambda ss: sum(tot[s + "_bets"] + tot[s + "_raises"] for s in ss)
    calls = lambda ss: sum(tot[s + "_calls"] for s in ss)
    folds = lambda ss: sum(tot[s + "_folds"] for s in ss)
    groups = [(STREET_NAMES[s], (s,)) for s in "pftr"] + [("total", ("f", "t", "r"))]
    for name, ss in groups:
        a, c = agg(ss), calls(ss)
        rows.append(f"{currency},af_{name},{a},{c},{'' if c == 0 else fixed2(a, c)}")
    for name, ss in groups:
        a, d = agg(ss), agg(ss) + calls(ss) + folds(ss)
        rows.append(f"{currency},afq_{name},{a},{d},{pct(a, d)}")
    return rows


def write_fixture(name, path):
    """Write the raw-string constant `name` of tests/common/mod.rs to `path`."""
    root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    src = open(os.path.join(root, "tests", "common", "mod.rs"), encoding="utf-8").read()
    marker = f'pub const {name}: &str = r#"'
    start = src.index(marker) + len(marker)
    text = src[start:src.index('"#;', start)]
    os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(text + "\n")
    print(f"wrote {name} to {path}")


def main():
    if len(sys.argv) == 4 and sys.argv[1] == "--write-fixture":
        write_fixture(sys.argv[2], sys.argv[3])
        return
    db = sys.argv[1]
    want_equity = "--equity" in sys.argv
    hands = load(db)
    by_id = {h["site_hand_id"]: h for h in hands}
    v36 = [h for h in hands if h["file"].startswith("GG")]
    v35 = [h for h in hands if h["file"].startswith("NLHRed15_")]
    print(f"hands={len(hands)} (v3.6 folder {len(v36)}, v3.5 file {len(v35)}), player-hands="
          f"{sum(len(h['players']) for h in hands)}, NULL positions="
          f"{sum(1 for h in hands for v in positions(h).values() if v is None)}")

    def hero_totals(hs):
        tot = collections.Counter()
        for h in hs:
            tot.update(counters(h)[0][hero(h)["name"]])
        return tot

    t36, t35 = hero_totals(v36), hero_totals(v35)
    print("\n## SPEC §4 (M13): Hero hands by position, v3.6 folder")
    pos = collections.Counter()
    for h in v36:
        pos[position_label(positions(h)[hero(h)["seat"]], h["max_seats"])] += 1
    print("  " + ", ".join(f"{k} {pos[k]}" for k in ("EP", "MP", "CO", "BTN", "SB", "BB", "?")))
    print("\n## SPEC §4 (M13): Hero M13 counters (v3.6 folder | v3.5 file)")
    for k in COUNTERS[:10] + [f"{s}_{a}" for s in STREETS for a in ("bets", "raises", "calls", "checks", "folds")]:
        print(f"  {k:<18} {t36[k]:>5} | {t35[k]}")
    print("\n## SPEC §5 (M14): Hero M14 counters, v3.6 folder")
    for k in COUNTERS[10:46]:
        print(f"  {k:<18} {t36[k]}")

    def results_row(hs, with_ev):
        n = len(hs)
        net = sum(hero(h)["net"] for h in hs)
        bbw_num = sum(hero(h)["net"] * 1_000_000 // h["bb"] for h in hs)  # exact: stakes divide
        assert all(hero(h)["net"] * 1_000_000 % h["bb"] == 0 for h in hs)
        r = collections.Counter()
        for h in hs:
            r.update({k: v for k, v in results(h)[hero(h)["name"]].items() if k != "net"})
        row = (f"CAD,{stakes(hs[0])},{n},{money(net)},{fixed2(net * 100, n * 1_000_000)},"
               f"{fixed2(bbw_num, 1_000_000)},{fixed2(bbw_num * 100, n * 1_000_000)},{money(r['rake'])},"
               f"{money(r['fees'])},{money(r['blinds'])},{money(r['straddles'])}")
        if with_ev:
            evn, allin, evbb = 0, 0, 0
            for h in hs:
                ev = all_in_ev(h)
                e = ev_net(h, ev)[hero(h)["name"]]
                evn += e
                evbb += e * 1_000_000 // h["bb"]
                if ev and hero(h)["name"] in ev["players"]:
                    allin += 1
            row += f",{allin},{money(evn)},{fixed2(evbb * 100, n * 1_000_000)}"
        return row

    print("\n## SPEC §6 (M15): results (v3.6 folder alone), then the v3.5 file alone")
    print("  " + results_row(v36, False))
    print("  " + results_row(v35, False))
    print("\n## SPEC §6 (M15): positions")
    agg = collections.defaultdict(collections.Counter)
    for h in v36:
        c, p = counters(h)
        me = hero(h)
        lab = position_label(p[me["seat"]], h["max_seats"])
        agg[lab].update(dict(hands=1, net=me["net"], bbw=me["net"] * 1_000_000 // h["bb"], **{
            k: c[me["name"]][k] for k in ("p_vpip", "p_pfr", "p_rfi", "p_rfi_opp")}))
    for lab in ("EP", "MP", "CO", "BTN", "SB", "BB", "?"):
        a = agg.get(lab)
        if a:
            print(f"  CAD,{lab},{a['hands']},{money(a['net'])},{fixed2(a['bbw'] * 100, a['hands'] * 1_000_000)},"
                  f"{pct(a['p_vpip'], a['hands'])},{pct(a['p_pfr'], a['hands'])},{pct(a['p_rfi'], a['p_rfi_opp'])}")
    print("\n## SPEC §6 (M15): sessions")
    sess = sessions(v36)
    for s in sess:
        net = sum(hero(h)["net"] for h in s)
        bbw = sum(hero(h)["net"] * 1_000_000 // h["bb"] for h in s)
        print(f"  CAD,{s[0]['table']},{s[0]['ts'].isoformat()},{s[-1]['ts'].isoformat()},{len(s)},"
              f"{money(net)},{fixed2(bbw * 100, len(s) * 1_000_000)}")
    print(f"  ({len(sessions(hands))} sessions over the whole database)")
    print("\n## SPEC §6 (M15): stats")
    for row in stats_rows(t36, "CAD"):
        print("  " + row)
    print("\n## docs/stats.md §7: worked examples")
    for hid in ("HD140650557", "HD145385563", "HD140650645"):
        h = by_id[hid]
        cs, p = counters(h)
        r = results(h)
        print(f"  {hid}")
        for pl in h["players"]:
            nz = sorted(k for k, v in cs[pl["name"]].items() if v)
            print(f"    {pl['name']:<9} {position_label(p[pl['seat']], h['max_seats']):<3} rake={r[pl['name']]['rake']}"
                  f" fees={r[pl['name']]['fees']} {nz}")
    if not want_equity:
        return

    print("\n## SPEC §7.2 (M16): exact equities")
    parse = lambda s: [((("23456789TJQKA".index(x[0])) << 2) | "cdhs".index(x[1])) for x in s.split()]
    for holes, board in ((("Ah As", "Kc Kd"), ""), (("Ac Kd", "Ah Kc"), ""),
                         (("As Ks", "Qh Qd", "Jc Tc"), ""), (("Ah Ad", "Kh Kd"), "Ks 7c 2d")):
        eq = exact_equity([parse(x) for x in holes], parse(board))
        print(f"  {' vs '.join(holes)} | {board or '-'} | " + ", ".join(f"{a}/{b}" for a, b in eq))
    print("\n## SPEC §7.3 (M16): EV Cashout hands (equity at the cashout street)")
    for hid in ("HD140653161", "HD140647649", "HD140642980", "HD145319803"):
        h = by_id.get(hid)
        if h is None:
            print(f"  {hid}: not in the database (see tools/oracles/README.md)")
            continue
        who, kind, amount, street = h["adjustments"][0]
        _, _, _, live = qualification(h)
        live = sorted(live, key=lambda p: p["name"] != who)
        num, den = exact_equity([p["cards"] for p in live], board_at(h, street))[0]
        pot_after = h["pot"] - sum(a for _, a in h["deductions"])
        won = sum(a for n, a in h["winners"] if n == who)
        cash = amount if kind == "EvCashoutReceived" else won + amount
        ratio = round_half_even_div(cash * den * 10_000, num * pot_after)
        print(f"  {hid} {street} {num}/{den} pot_after={money(pot_after)} cashout={money(cash)}"
              f" ratio=0.{ratio:04d}")
    print("\n## SPEC §7.3/§7.4 (M16): EV_CASHOUT_RECEIVED columns")
    h = by_id.get("HD145319803")
    if h:
        ev = all_in_ev(h)
        nets = ev_net(h, ev)
        for name, (num, den, _) in ev["players"].items():
            print(f"  {name} equity {num}/{den} ev_net_micros={nets[name]}")
    print("\n## SPEC §7.4 (M16): qualification and Hero's qualifying hands")
    corpus = v36 + v35
    q = [h for h in corpus if qualification(h)[0]]
    print(f"  qualifying: {len(q)} ({sum(1 for h in q if h in v36)} v3.6, {sum(1 for h in q if h in v35)} v3.5)")
    print("  side pots: " + ", ".join(h["site_hand_id"] for h in corpus if qualification(h)[1] == "side pots"))
    for h in corpus:
        ev = all_in_ev(h)
        if ev and hero(h)["name"] in ev["players"]:
            me = hero(h)
            num, den, _ = ev["players"][me["name"]]
            print(f"  {h['site_hand_id']} {ev['street']} {num}/{den} pot_after={money(ev['pot_after'])}"
                  f" net={money(me['net'])} ev_net={money(ev_net(h, ev)[me['name']])}")
    print("\n## SPEC §7.5 (M16): equity-adjusted results")
    for f in sorted({h["file"] for h in corpus}):
        hs = [h for h in corpus if h["file"] == f]
        qq = [h for h in hs if qualification(h)[0]]
        hin = sum(1 for h in qq if hero(h)["name"] in all_in_ev(h)["players"])
        print(f"  {f[:24]:<24} qualifying={len(qq)} hero_in={hin} net={money(sum(hero(h)['net'] for h in hs))}"
              f" ev_net={money(sum(ev_net(h, all_in_ev(h))[hero(h)['name']] for h in hs))}")
    print("  " + results_row(v36, True))
    print("  sessions ev_net: " + ", ".join(
        money(sum(ev_net(h, all_in_ev(h))[hero(h)["name"]] for h in s)) for s in sess))


if __name__ == "__main__":
    main()
