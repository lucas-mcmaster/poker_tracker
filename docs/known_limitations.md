# Known Limitations (Deliberately Deferred)

Living record of scope boundaries and intentionally deferred features for `poker_tracker` v3.6+.
Derived from `docs/BLUEPRINT.md` §8 with updates from the v3.6 specification (`docs/SPEC_v3.6.md`).

---

- **Derived statistics** (positions, VPIP/PFR, all-in EV, bb/100) belong to a later stats layer.
  The schema stores everything they need. Compute bb-normalized figures per currency, or convert
  explicitly. Straddles are forced posts: exclude them from VPIP and PFR. Heads-up has two
  positions: the button is the small blind (SB, code 9) and the other player the BB (code 8).
  Definitions are in `docs/stats.md`; the work is `docs/SPEC_v3.7.md` (M13–M16).
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

