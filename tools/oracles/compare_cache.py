#!/usr/bin/env python3
"""Compare every row of a poker_tracker database's hand_player_stats cache with the oracle.

Reviewer tool for SPEC_v3.7. The crate fills the cache; v3_7_oracle.py recomputes every
counter, position, money column (M15) and all-in equity column (M16) from the stored canonical
hands. Every column present in the cache is compared, for every player-hand, not only Hero's
totals.

Usage (see README.md):
    python3 -I tools/oracles/compare_cache.py <db>
    <venv>/bin/python -I tools/oracles/compare_cache.py <db> --equity   # also ev_* (eval7, ~75 s)

Exit status 0 when every compared value matches, 1 otherwise.
"""
import collections
import importlib.util
import os
import sqlite3
import sys

sys.dont_write_bytecode = True  # never leave __pycache__ in the repository

_spec = importlib.util.spec_from_file_location(
    "v3_7_oracle", os.path.join(os.path.dirname(os.path.abspath(__file__)), "v3_7_oracle.py"))
o = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(o)

MONEY = {  # cache column -> key of v3_7_oracle.results()
    "rake_attributed_micros": "rake",
    "fees_attributed_micros": "fees",
    "blinds_posted_micros": "blinds",
    "straddles_posted_micros": "straddles",
}
EQUITY = ("ev_qualified", "ev_equity_num", "ev_equity_den", "ev_net_micros")


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    db, want_equity = sys.argv[1], "--equity" in sys.argv[2:]
    conn = sqlite3.connect(db)
    conn.row_factory = sqlite3.Row
    cols = [r[1] for r in conn.execute("PRAGMA table_info(hand_player_stats)")]
    if not cols:
        sys.exit("no hand_player_stats table in this database")
    version = conn.execute(
        "SELECT version FROM derived_cache WHERE name = 'hand_player_stats'").fetchone()
    known = set(o.COUNTERS) | set(MONEY) | set(EQUITY) | {"hand_id", "player_id", "position"}
    cache_cols = [c for c in cols if c not in ("hand_id", "player_id")]
    unknown = [c for c in cache_cols if c not in known]
    ev_cols = [c for c in EQUITY if c in cols]
    if ev_cols and not want_equity:
        print(f"note: {len(ev_cols)} ev_* columns present; pass --equity to compare them")
    compared = [c for c in cache_cols if c in known and (c not in EQUITY or want_equity)]
    print(f"stats version {version[0] if version else None}; {len(cache_cols)} cache columns; "
          f"comparing {len(compared)}; unknown to the oracle: {unknown or 'none'}")

    names = {r["id"]: r["screen_name"] for r in conn.execute("SELECT id, screen_name FROM players")}
    rows = {(r["hand_id"], names[r["player_id"]]): dict(r)
            for r in conn.execute("SELECT * FROM hand_player_stats")}
    hands = o.load(db)
    diffs, examples, player_hands = collections.Counter(), [], 0
    for h in hands:
        counters, pos = o.counters(h)
        money = o.results(h) if any(c in MONEY for c in compared) else {}
        ev = o.all_in_ev(h) if want_equity and ev_cols else None
        ev_nets = o.ev_net(h, ev) if want_equity and ev_cols else {}
        for p in h["players"]:
            player_hands += 1
            name = p["name"]
            row = rows.get((h["id"], name))
            if row is None:
                diffs["<missing row>"] += 1
                continue
            for c in compared:
                if c == "position":
                    want = pos[p["seat"]]
                elif c in MONEY:
                    want = money[name][MONEY[c]]
                elif c in EQUITY:
                    q = ev is not None and name in ev["players"]
                    num, den = (ev["players"][name][:2]) if q else (0, 1)
                    want = {"ev_qualified": int(q), "ev_equity_num": num, "ev_equity_den": den,
                            "ev_net_micros": ev_nets[name]}[c]
                else:
                    want = counters[name][c]
                if row[c] != want:
                    diffs[c] += 1
                    if len(examples) < 15:
                        examples.append(f"  {h['site_hand_id']} {name} {c}: cache {row[c]}, oracle {want}")
    extra = len(rows) - (player_hands - diffs["<missing row>"])
    if extra:
        diffs["<row with no hand_players entry>"] = extra
    print(f"{player_hands} player-hands, {len(rows)} cache rows; differences: "
          f"{dict(diffs) if diffs else 'none'}")
    print("\n".join(examples))
    sys.exit(1 if diffs else 0)


if __name__ == "__main__":
    main()
