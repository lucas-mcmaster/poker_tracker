#!/usr/bin/env python3
"""Compare two poker_tracker databases: every table's rows and the schema text.

Reviewer tool (SPEC_v3.8 §4, M17.1 "Done when"). Two imports of the same folders by two builds
must give the same rows, apart from run ids and times, and the same `sqlite_master.sql` for
every table, index and view, byte for byte (a refactor that regenerates a `CREATE TABLE` must
not drop or reword a constraint).

Usage:
    python3 -I tools/oracles/db_compare.py <a.sqlite> <b.sqlite>

Exit status 0 when both databases match, 1 otherwise.
"""
import hashlib
import sqlite3
import sys

SKIP = {"import_runs", "sqlite_sequence", "derived_cache"}
DROP = {"import_run_id", "built_at", "started_at"}


def schema(c):
    return {(t, n): s for t, n, s in c.execute(
        "SELECT type, name, sql FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name")}


def rows(c):
    out = {}
    for (n,) in c.execute("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name"):
        if n in SKIP:
            continue
        cols = [r[1] for r in c.execute(f"PRAGMA table_info({n})") if r[1] not in DROP]
        data = sorted(c.execute(f"SELECT {','.join(cols)} FROM {n}").fetchall(), key=repr)
        out[n] = (len(data), hashlib.sha256(repr(data).encode()).hexdigest()[:16])
    return out


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    a, b = sqlite3.connect(sys.argv[1]), sqlite3.connect(sys.argv[2])
    same = True
    ra, rb = rows(a), rows(b)
    for n in sorted(set(ra) | set(rb)):
        ok = ra.get(n) == rb.get(n)
        same &= ok
        print(f"rows   {n:22} {ra.get(n)} {rb.get(n)} {'same' if ok else 'DIFFERENT'}")
    sa, sb = schema(a), schema(b)
    for key in sorted(set(sa) | set(sb)):
        ok = sa.get(key) == sb.get(key)
        same &= ok
        print(f"schema {key[0]:5} {key[1]:38} {'same' if ok else 'DIFFERENT'}")
        if not ok:
            print(f"  a: {sa.get(key)!r}\n  b: {sb.get(key)!r}")
    print("databases match" if same else "databases DIFFER")
    sys.exit(0 if same else 1)


if __name__ == "__main__":
    main()
