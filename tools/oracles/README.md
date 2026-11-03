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

If its output and `docs/stats.md` disagree, the script has a bug; if it and the crate disagree,
find which one departs from `docs/stats.md`.

It also runs on a database holding only the v3.6 folder (the database SPEC_v3.7 §6 pins its
reports on); sections for the missing file then print a note.

Last run: 2026-11-02, after the pre-M13 amendments (SPEC_v3.7 §11: walks and PFR opportunities,
session `platform` and `duration_seconds`), on the 1,283 hands of `tests/fixtures/ggpoker/` plus
HD145319803, and on the v3.6 folder alone (Python 3.13, eval7 0.1.11).
