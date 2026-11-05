# Oracles

Independent re-implementations used to produce the expected numbers in the specs. They share no
code with the crate, so a test that matches them checks the Rust implementation against a second
reading of the same definitions.

## `v3_7_oracle.py` (SPEC_v3.7, M13–M16)

Implements `docs/stats.md`: positions, the stat counters, results (rake and fee attribution,
blinds), table sessions and exact all-in equity. It reads a `poker_tracker` SQLite database, so
the hands are parsed by the crate but every statistic is computed here. Its output is labelled
with the SPEC_v3.7 section that pins each number.

```
# 1. A database with every real hand, plus EV_CASHOUT_RECEIVED (it is only in tests/common/mod.rs)
python3 -I tools/oracles/v3_7_oracle.py --write-fixture EV_CASHOUT_RECEIVED /tmp/oracle_extra/hand.txt
cargo run --release --locked -- tests/fixtures/ggpoker --ggpoker-currency CAD --db /tmp/oracle.sqlite
cargo run --release --locked -- /tmp/oracle_extra --ggpoker-currency CAD --db /tmp/oracle.sqlite

# 2. M13–M15 (standard library only)
python3 -I tools/oracles/v3_7_oracle.py /tmp/oracle.sqlite

# 3. M16 needs the eval7 evaluator. `python3 -I` ignores user site-packages, so use a venv:
python3 -m venv /tmp/oracle-venv && /tmp/oracle-venv/bin/pip install eval7
/tmp/oracle-venv/bin/python -I tools/oracles/v3_7_oracle.py /tmp/oracle.sqlite --equity   # ~1.5 min

# 4. Clean up
rm -rf /tmp/oracle.sqlite* /tmp/oracle_extra /tmp/oracle-venv
```

## `compare_cache.py` (reviewer check, M13–M16)

Compares **every row** of a database's `hand_player_stats` with the oracle: each counter,
`position`, the M15 money columns and, with `--equity`, the M16 `ev_*` columns, for every
player-hand (not only Hero's totals). Columns the oracle does not know are listed. Exit status
`0` when everything matches.

```
python3 -I tools/oracles/compare_cache.py /tmp/oracle.sqlite
/tmp/oracle-venv/bin/python -I tools/oracles/compare_cache.py /tmp/oracle.sqlite --equity   # ~1 min
```

If its output and `docs/stats.md` disagree, the script has a bug; if it and the crate disagree,
find which one departs from `docs/stats.md`.

It also runs on a database holding only the v3.6 folder (the database SPEC_v3.7 §6 pins its
reports on); sections for the missing file then print a note.

Last run: 2026-11-03, at `f385c63` (v3.7 complete, `STATS_VERSION` 4), on the 1,283 hands of
`tests/fixtures/ggpoker/` plus HD145319803 (Python 3.13, eval7 0.1.11): `compare_cache.py
--equity` found no difference over 6,846 player-hands × 77 columns, and `v3_7_oracle.py --equity`
printed every number SPEC_v3.7 pins.

## `v3_8_oracle.py` (SPEC_v3.8, M17)

Adds what v3.8 needs on top of `v3_7_oracle.py`, whose definitions it reuses: the filter document
(`docs/SPEC_v3.8.md` §5) and the filter definitions of `docs/stats.md` §9, the player selection,
and the four CSV reports for any selection. Filters are evaluated in Python on the oracle's own
values, never on `hand_player_stats`. Results and sessions need `eval7` (their `ev_*` columns).

```
# Self-check: with no filter, the v3.6 folder's four reports equal tests/golden/v3_6_*.csv
cargo run --release --locked -- tests/fixtures/ggpoker/v3_6 --ggpoker-currency CAD --db /tmp/v36.sqlite
for k in results positions sessions stats; do
  /tmp/oracle-venv/bin/python -I tools/oracles/v3_8_oracle.py /tmp/v36.sqlite --report $k | cmp - tests/golden/v3_6_$k.csv
done

# One report for any selection
/tmp/oracle-venv/bin/python -I tools/oracles/v3_8_oracle.py /tmp/v36.sqlite --report stats \
  --filter tests/fixtures/filters/late_position.json [--player GGPoker/c7bdd1e4] [--stakes 0.05/0.1]

# The SPEC_v3.8 golden cases: tests/golden/v3_8/<case>/case.json -> <case>/<kind>.csv
/tmp/oracle-venv/bin/python -I tools/oracles/v3_8_oracle.py /tmp/v36.sqlite --check-cases tests/golden/v3_8   # ~1 min
```

`--write-cases` writes the CSVs instead of comparing them; the reviewer used it to make the goldens.

Last run: 2026-11-04, at `50adfe8` (v3.7.1, `STATS_VERSION` 4), on the 989 hands of the v3.6
folder: the four `v3_6_*.csv` goldens reproduced byte for byte, and the 23 cases of
`tests/golden/v3_8/` were written. The two catalog files beside them (`catalog_m17_3.json`,
`catalog_m17_4.json`) were written by the reviewer from `docs/SPEC_v3.8.md` M17.3.4, not by this script.

## `db_compare.py` (reviewer check)

Compares two databases table by table (rows, ignoring run ids and times) and compares the
`sqlite_master.sql` of every table, index and view byte for byte. Build the base commit's release
binary in a `git worktree`, import the same folders into a second database, then:

```
python3 -I tools/oracles/db_compare.py /tmp/base.sqlite /tmp/new.sqlite   # exit 0 = identical
```

## `equity_spots.py` (the `exact_equity` regression fixture)

Writes `tests/fixtures/equity/eval7_spots.txt`: 2,130 spots fixed by a seed (1,000 random and
1,000 tie-provoking river spots with 2–6 players, 100 turn and 30 flop spots), each with its
exact equities from `eval7`. `tests/equity.rs::exact_equity_matches_eval7_on_2130_fixed_spots`
compares the crate with them, so the evaluator's comparisons are pinned by `cargo test` without
needing `eval7` there. The larger 50,524-spot differential run of `HANDOFF_v3.7_review.md`
Appendix A found no mismatch either.

```
/tmp/oracle-venv/bin/python -I tools/oracles/equity_spots.py tests/fixtures/equity/eval7_spots.txt
```

The output is deterministic: rerunning it must leave the fixture unchanged (`git diff --exit-code`).
