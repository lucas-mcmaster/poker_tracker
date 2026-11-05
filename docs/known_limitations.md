# Known Limitations (Deliberately Deferred)

Living record of scope boundaries, intentionally deferred features and the planned milestones.
It started from `docs/history/BLUEPRINT.md` §8.

---

- **Derived statistics.** The derived cache (M13), opportunity counters (M14), results, sessions,
  and reports (M15), and exact all-in equity (M16) are implemented. Filters are v3.8
  (`docs/SPEC_v3.8.md`); all-in equity with side pots and the stats of `docs/stats.md` §8 remain
  deferred.
- **Session durations.** Session durations understate the time at the table by about one hand (hand
  histories record start times only). Elapsed times are measured on the timestamps as printed when
  a table's hands do not all state a UTC offset (GGPoker states none), so a session that crosses a
  daylight-saving change is off by an hour there.
- An open that starts while another process rebuilds the statistics cache, or commits an import
  batch (every open, `--report` included, takes the write lock briefly to check migrations),
  waits up to 5 s and then fails with a lock error; run it again.
- A stored hand that cannot be loaded makes the cache rebuild, and therefore every open by a
  v3.7 build, fail with an error naming the hand, until the row is repaired outside
  `poker_tracker`.
- **Rebuild time.** Exact all-in equity enumerates every runout, so it dominates deriving the cache:
  a heads-up preflop all-in takes well under 0.1 s in a release build, and the 1,283 corpus hands
  plus HD145319803 (1,284 hands) rebuild in under 1 s. A full rebuild grows with the number of
  preflop all-ins (about 10 minutes for a million hands at the corpus's rate) and holds the write
  lock meanwhile.
- **Sessions across time bases.** Sessions are listed by their printed `start`, also when one
  table is timed in UTC and another as printed (`docs/stats.md` §6.1), so such a listing can be
  out of real-time order. GGPoker states no UTC offset, so it cannot happen with its exports.
- **`--out`** replaces an existing file without asking.
- **Currency conversion** (e.g. CAD and USD results combined) is a reporting concern. Storage
  never mixes currencies.
- **Game variants.** The model is Hold'em: two hole-card slots, NL cash. Omaha, other limits and
  tournaments are the main future model extensions: hole-card arity, `GameType` and
  `BettingLimit` variants, and chips rather than currency.
- **EV Cashout.** *EV Cashout* is supported on both sides (`EvCashoutRiskPaid`,
  `EvCashoutReceived`). A cashout in a split or multi-run pot has not been observed and is
  quarantined with `UnsupportedAdjustment`. The split-pot test is "more than one payout line",
  so a cashout hand in which one player collects a main and a side pot is quarantined too.
- **All-in blinds for less than the stake.** The validator accepts them, but most sites make
  the others call the full big blind, which the replay does not model; the GGPoker parser
  refuses such hands until a real sample shows how GGPoker prints them. A blind of exactly the
  remaining stack is supported.
- **Side-pot structure.** GGPoker prints every payout as `collected $x from pot`. The validator
  bounds what each player can win, but main and side pot amounts, if ever needed, must be
  computed from contributions.
- **Not yet observed on GGPoker** (rejected until a sample is added):
  - antes, bomb pots, all-in or short straddles, three or more straddles, a second straddle
    by the same player, heads-up straddles;
  - heads-up hands with a missed or dead blind, or where the button does not post the small
    blind;
  - `mucks hand`, seat-change and sit-out lines;
  - an EV Cashout chosen before the flop (the site-neutral core accepts one), and amounts with
    more than two decimals;
  - non-`$` stakes;
  - Omaha and other limits.
- **Timestamps** from GGPoker carry no time zone (`utc_offset_seconds = None`), so ordering
  hands across different sites is approximate until a zone is configured or stated.
- **Player identity.** The `players` table is keyed by `(platform, screen_name)`.
  - GGPoker opponent hashes are per table session. Across 10 tables (101 hashes) none recurred,
    even at the same table name on the same day, so opponent-level features cannot span sessions.
  - Linking the hero's accounts across sites belongs to a future accounts layer.
- **Re-parsing from the database.** Raw text is stored for every hand and every quarantined
  hand, but there is no command yet that re-parses it; re-import the original folder instead.
- **Schema history.** Migration 1 is the v3.4 baseline and migration 2 the v3.5 upgrade. A v3.4
  database upgrades in place; databases created by v3.3-or-earlier code must be deleted and
  re-imported once.
- **The betting replay assumes No-Limit.** Pot-Limit and Fixed-Limit support would add
  bet-size rules.

---

## Planned milestones (do not build before a spec covers them)

v3.8 (M17.1–M17.6, filters) is specified in `docs/SPEC_v3.8.md`. The rest is a plan whose order
can change with the owner's priorities; each item gets a spec before it is built.

- **v3.9 — cache v5 and application readiness** (planned, flexible).
  - The UI technology is chosen; the library's long operations get the shape it needs.
  - Opening a database never rebuilds the cache by itself: it reports a stale cache, and a
    rebuild runs on request with progress and cancellation (today the open rebuilds under the
    write lock, about 10 minutes at a million hands, with no progress).
  - All-in equity is kept across rebuilds, keyed on its inputs (the live players' hole cards and
    the all-in board) under its own version, so a `STATS_VERSION` bump no longer recomputes it.
    Keying on the raw text's hash would not do: a re-parse (M19) keeps the text and can change
    the hand. This changes `AGENTS.md` §3.10 and the cache versioning rules.
  - The rebuild reads each hand's players once (today a second query per hand re-reads them).
  - Final hand and board texture: definitions in `docs/stats.md` first, then a per-hand cache
    table and `STATS_VERSION` 5, then their filters.
  - Money filters ("amount won at least 5 bb") and, if wanted, session filters (session net,
    duration; they need a two-pass selection).
  - Display labels for stats, counters and filters belong to the UI (localization), not here.
- **M19 — Database management.**
  - Re-parse stored raw text and quarantined hands after a parser upgrade (this closes the
    re-parsing limitation above). It needs a "replace the hand stored under this id" operation.
  - Export selected hands as raw text (a filter selects them).
  - Purge by date or run.
  - SQLite online backup.
  - Hero aliases across sites (the accounts layer).
  - Optional "my currency" conversion with a stored rate per session (Game Currency vs My
    Currency, as in PT4), never mixing stored amounts.
- **Deferred further:**
  - M18, a leak report: Hero's stats by position against reference ranges and the pool (every
    non-hero player-hand together).
  - M20, hand review: tags, notes, an HTML replayer.
  - A cross-check of the pinned stats against PT4 on the same exports, by the owner for final
    testing. PT4's rake attribution must be set to "contributed".
