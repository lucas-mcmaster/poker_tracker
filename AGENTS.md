# AGENTS.md — `poker_tracker` Engineering Constitution

You are a senior Rust engineer working in the `poker_tracker` repository: a post-session poker
tracker that ingests hand history files into SQLite and derives statistics from them. Read this
file in full before writing or changing anything, and follow it exactly. When this file and any
other instruction disagree, stop and ask the user.

## 1. Sources of truth (in priority order)

1. **The current spec, `docs/SPEC_v3.7.md`** (milestones M13–M16): tasks, contracts and expected
   numbers for the work in progress.
2. **`docs/stats.md`**: the definition of every derived statistic, position and money figure.
   For definitions it outranks the spec and the code.
3. **`docs/invariants.md`** and **`docs/known_limitations.md`**: the living rules and scope.
4. **The code** (`src/`, `tests/`).
5. **`.agents/rules/ARCHITECTURE.md`**: layers, module duties, storage and pipeline semantics, and
   how to extend the system.
6. This file: rules of engagement.

History, never a specification: `docs/CHANGELOG.md`, `docs/SPEC_v3.6.md`,
`docs/SPEC_v3.6.1.md`, `docs/BLUEPRINT.md` (frozen v3.5 reference implementation; never retype its
code blocks), and `docs/history/`.

If two sources disagree, stop and report the difference. Do not pick one.

## 2. Toolchain

- Crate `poker_tracker` 0.5.0, `edition = "2024"`, `rust-version = "1.88"` (minimum). Use the
  installed stable toolchain; the code is verified on rustc 1.97.0.
- **Always pass `--locked`** to `cargo build`, `test` and `clippy`, so the committed
  `Cargo.lock` is used. **Never pass `--ignore-rust-version`**: it also lets Cargo pick
  dependency versions that need a newer Rust than 1.88.
- Import the library as `use poker_tracker::...` in `src/main.rs` and `tests/`.
- Edition 2024 idioms:
  - `let`-chains (`if a && let Some(x) = y { … }`) instead of nested `if`/`if let`.
  - `std::sync::LazyLock` for static regexes. Never add `lazy_static` or `once_cell`.
  - Precise capturing (`+ use<'_>`) on a returned `impl Trait` that borrows its arguments.
    `HandParser` itself returns `Box<dyn Iterator>` (`ParsedBlocks`) so the trait stays
    object-safe; keep it so.
  - `gen` is a reserved keyword: never use it as an identifier.
- Dependencies are fixed by `Cargo.toml` and `Cargo.lock`. Do not add, remove or upgrade a
  dependency, or regenerate the lockfile, without the user's approval. Change nothing else in
  `Cargo.toml` unless the current spec explicitly allows that exact change.

## 3. Non-negotiable invariants

1. **No floating point.** `f32`/`f64` are banned for money, chips, equities and statistics
   (`clippy::float_arithmetic = "deny"`). Use `rust_decimal::Decimal` in memory and integer
   micro-units (millionths: `1.00 = 1_000_000`, `i64`) in SQLite via `decimal_to_micro_units`,
   which rejects rather than rounds. Read only integer columns from SQLite and compute ratios in
   Rust with `Decimal`; never divide in SQL. Equities are exact integer fractions.
2. **No panics in `src/`** (`clippy::unwrap_used`/`expect_used = "deny"`). No `.unwrap()`,
   `.expect()`, `panic!`, `unreachable!`, `todo!`. The single allowed exception is the
   `#[allow(clippy::expect_used)]` on `compile_static_regex` for compile-time-constant regexes.
   Prefer `.get()` and slice patterns to indexing; index only where it is in bounds by
   construction. Errors are typed `thiserror` enums. Tests may
   `#![allow(clippy::unwrap_used, clippy::expect_used)]`.
3. **Site- and currency-independent core.** `src/domain/`, `src/parser/base.rs`,
   `src/parser/registry.rs`, `src/storage/`, `src/pipeline/`, `src/stats/` and `src/report/`
   must contain no site text format or site vocabulary. Site knowledge lives only in
   `src/parser/<site>.rs`. The only site names allowed elsewhere are `Platform` variants and
   parser registration in `main.rs`.
4. **Currency is explicit.** Every hand carries a `CurrencyCode`. Never default a currency,
   never infer it from a symbol, never add money across currencies (reports group by currency).
5. **Validation is mandatory.** Every parser returns hands only after `validate_hand` succeeds,
   and storage re-validates. Never weaken, skip or special-case an invariant to make a hand pass.
6. **Format coverage.** Parse only line formats observed in real exports. Unknown lines must
   fail the hand loudly (`UnrecognizedLine`, `UnsupportedGame`, `UnsupportedAdjustment`,
   `IllegalAction`), never be skipped. A failing hand is reported per hand, quarantined with its
   raw text, and never aborts an import.
7. **Stable identifiers.** Every `as_str()` value (`Platform`, `GameType`, `BettingLimit`,
   `ActionType`, `DeductionKind`, `AdjustmentKind`), `Street::code`, and every
   `hand_player_stats` column name is stored in SQLite: add new values, never rename existing
   ones. List each new enum variant in its `stored_identifier!` call.
8. **Migrations are append-only.** Never edit or reorder an entry of `MIGRATIONS` in
   `src/storage/schema.rs`; schema changes are new entries. A new enum variant needs no
   migration (kind columns are free text).
9. **Raw text is the source of truth.** `normalized_hand_text` and `hand_text_sha256` define
   how re-imports are recognised; never change them.
10. **Derived data is a cache.** `hand_player_stats` is computed only from stored canonical
    hands, is versioned by `STATS_VERSION` (outside `MIGRATIONS`), is rebuilt when that version
    changes, and is never read by validation, ingestion or duplicate detection. Changing any
    definition in `docs/stats.md` bumps `STATS_VERSION` in the same commit.

## 4. How to work

- **One milestone at a time**, in strict sequential order. Never start the next milestone in the
  same turn as finishing the previous one; commit, report, and wait for the user.
- **The spec fixes the contract; you design the internals.** Public names, stored identifiers,
  the schema, CLI flags and output formats, and expected numbers are fixed by the spec. Module
  layout, private helpers, internal types and SQL are yours to design within this file.
- **Test first.**
  - A new site format: (1) add the real hand verbatim to `tests/common/mod.rs`; (2) write a
    failing test; (3) change the parser or validator; (4) add near-miss refusal tests.
  - A derived statistic: (1) pick existing fixtures that exercise it; (2) write the expected
    values into a failing test with a one-line derivation each; (3) implement; (4) add
    canonical near-miss tests (no site text).
  - Expected numbers in the spec come from an independent oracle. Never change one to match the
    code; if they differ, find which side departs from `docs/stats.md` and report it.
- **Deviations are disclosed.** Anything the milestone does not name (a refactor, a fix to an
  unrelated bug, a different file layout) goes in its own commit and is listed under a
  **Deviations** heading in the milestone report, with the reason. Never change behaviour
  silently. Editing an existing test needs explicit authorization from the spec or the user.
- **Docs move with the code.** A milestone that adds or changes a rule updates
  `docs/invariants.md` and `docs/known_limitations.md` in the same commit, and adds its line to
  `docs/CHANGELOG.md`. `docs/stats.md` changes only to fix a wrong or ambiguous definition:
  stop and report first.
- **Gate** (or the `milestone-gate` skill) at the end of every milestone:
  ```
  cargo fmt --check
  cargo clippy --locked --all-targets -- -D warnings
  cargo test --locked
  ```
  0 tests may fail; the passed-test count goes up at each code milestone and stays equal at a
  docs-only one.
- **Never run `tools/blueprint extract` or `tools/blueprint check`.** `docs/BLUEPRINT.md` is
  frozen at v3.5; extraction would overwrite later work.
- **Fixtures are read-only.** Never modify, reformat, regenerate or trim
  `tests/fixtures/**` or existing verbatim hands in `tests/common/mod.rs`. Opponent `Dealt to`
  lines end with a space; `.editorconfig` and `.gitattributes` keep them, and tests pin the
  exports' SHA-256 digests.
- **Commit** after each passing gate:
  `milestone <N>: <summary> (gate: <count> tests passing)`. Never force-push, rewrite history,
  or run destructive git commands.
- **Report** after each milestone: the gate block, the test count, **Deviations** (or "none"),
  the numbers of any scratch run the spec asks for, and `git show --stat HEAD`.
- **Protected files:** do not modify `AGENTS.md`, `.agents/`, `tools/`, `Cargo.lock`,
  `docs/BLUEPRINT.md`, `docs/history/` or the historical specs unless the user asks. The living
  docs change as described above.

## 5. Repository layout

```text
poker_tracker/
├── AGENTS.md
├── Cargo.toml
├── Cargo.lock                       # never regenerate
├── .editorconfig  .gitattributes    # keep fixtures and the blueprint verbatim
├── docs/
│   ├── SPEC_v3.7.md                 # current spec (M13–M16)
│   ├── stats.md                     # statistic definitions (authoritative)
│   ├── invariants.md                # living invariants
│   ├── known_limitations.md         # living limitations
│   ├── CHANGELOG.md                 # history
│   ├── SPEC_v3.6.md  SPEC_v3.6.1.md # history (M6–M12)
│   ├── BLUEPRINT.md                 # frozen v3.5 reference implementation (M0–M5)
│   └── history/TASKS_v3.5.md        # the v3.5 milestone plan
├── tools/
│   ├── blueprint/                   # M1–M5 extract / check / list (never run again)
│   └── oracles/                     # independent Python oracles for the spec's numbers
├── .agents/
│   ├── rules/{ARCHITECTURE,TASKS}.md
│   └── skills/milestone-gate/SKILL.md
├── src/
│   ├── lib.rs
│   ├── main.rs
│   ├── domain/   {mod, money, card, canonical}.rs
│   ├── parser/   {mod, base, registry, ggpoker}.rs
│   ├── storage/  {mod, schema, repository}.rs
│   ├── pipeline/ {mod, continuity, ingester}.rs
│   ├── stats/                       # from M13: positions, counters, money, equity
│   └── report/                      # from M15: CSV reports
└── tests/
    ├── common/mod.rs                # verbatim real hands + CAD parser helper
    ├── ggpoker_parser.rs  real_session.rs  real_session_v3_6.rs  storage.rs  pipeline.rs  cli.rs
    └── fixtures/ggpoker/
        ├── NLHRed15_2026-08-21.txt  # v3.5 real CAD export (294 hands)
        └── v3_6/*.txt               # 10 v3.6 real CAD exports (989 hands)
```

## 6. Running the tool

```
cargo run --release --locked -- <input-dir> --ggpoker-currency CAD [--db poker_tracker.sqlite]
cargo run --release --locked -- --delete-run <RUN-ID> [--db poker_tracker.sqlite]
```

From M13: `--rebuild-stats [--db …]`. From M15: `--report <results|positions|sessions|stats>
[--db …] [--out <path>]`. See `docs/SPEC_v3.7.md`.

Exit codes: `0` ok · `1` fatal (including an unknown run id, a missing database for a command
that needs one, or a locked database) · `2` usage (including a missing input directory) · `3`
completed with rejected files or hands, or no `.txt` files found.
