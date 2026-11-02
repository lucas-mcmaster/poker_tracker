# AGENTS.md — `poker_tracker` Engineering Constitution

You are a senior Rust engineer working in the `poker_tracker` repository: a post-session poker
tracker whose backend ingests hand history files into SQLite. Read this file in full before
writing or changing anything, and follow it exactly. When this file and any other instruction
disagree, stop and ask the user.

## 1. Sources of truth (in priority order)

1. **For Milestones 6–11 (v3.6):** **`docs/SPEC_v3.6.md`** and the repository code (`src/`,
   `tests/`) are the source of truth. Follow the exact milestone tasks (M6 to M11) in
   `docs/SPEC_v3.6.md`.
2. **For Milestones 1–5 (v3.5 baseline, already completed):** **`docs/BLUEPRINT.md`** is the
   frozen v3.5 reference implementation (~10,000 lines). Read sections 0–8 (~530 lines) as
   needed for baseline context; never read its code blocks to retype them.
3. **`.agents/rules/ARCHITECTURE.md`** (and, from M11 onward, **`docs/invariants.md`** and
   **`docs/known_limitations.md`**): the design, layers, domain model, parser contract,
   invariants, storage semantics, and extension rules.
4. **`.agents/rules/TASKS.md`**: the v3.5 milestone plan (M0–M5).
5. This file: rules of engagement.

`docs/CHANGELOG.md` is history only, never a specification.

## 2. Toolchain

- Crate `poker_tracker` 0.5.0, `edition = "2024"`, `rust-version = "1.88"` (minimum). Use the
  installed stable toolchain; the blueprint was verified on rustc 1.97.0.
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
  dependency, or regenerate the lockfile, without the user's approval.

## 3. Non-negotiable invariants

1. **No floating point.** `f32`/`f64` are banned for money, chips and statistics
   (`clippy::float_arithmetic = "deny"`). Use `rust_decimal::Decimal` in memory and integer
   micro-units (millionths: `1.00 = 1_000_000`, `i64`) in SQLite via `decimal_to_micro_units`,
   which rejects rather than rounds. In SQL, divide `*_micros` columns by `1e6`.
2. **No panics in `src/`** (`clippy::unwrap_used`/`expect_used = "deny"`). No `.unwrap()`,
   `.expect()`, `panic!`, `unreachable!`, `todo!`. The single allowed exception is the
   `#[allow(clippy::expect_used)]` on `compile_static_regex` for compile-time-constant regexes.
   Index only where the index is in bounds by construction. Errors are typed `thiserror` enums
   (`ParseError`, `StorageError`, `PipelineError`, `DetectionError`, `CurrencyParseError`,
   `UnknownIdentifier`). Tests may `#![allow(clippy::unwrap_used, clippy::expect_used)]`.
3. **Site- and currency-independent core.** `src/domain/`, `src/parser/base.rs`,
   `src/parser/registry.rs`, `src/storage/` and `src/pipeline/` must contain no site text
   format or site vocabulary. Site knowledge lives only in `src/parser/<site>.rs`. The only
   site names allowed elsewhere are `Platform` variants and parser registration in `main.rs`.
4. **Currency is explicit.** Every hand carries a `CurrencyCode`. Never default a currency,
   never infer it from a symbol, never add money across currencies.
5. **Validation is mandatory.** Every parser returns hands only after `validate_hand` succeeds,
   and storage re-validates. Never weaken, skip or special-case an invariant to make a hand pass.
6. **Format coverage.** Parse only line formats observed in real exports. Unknown lines must
   fail the hand loudly (`UnrecognizedLine`, `UnsupportedGame`, `UnsupportedAdjustment`), never
   be skipped. A failing hand is reported per hand, quarantined with its raw text, and never
   aborts an import.
7. **Stable identifiers.** Every `as_str()` value (`Platform`, `GameType`, `BettingLimit`,
   `ActionType`, `DeductionKind`, `AdjustmentKind`) and `Street::code` is stored in SQLite: add
   new values, never rename existing ones. List each new variant in its `stored_identifier!`
   call.
8. **Migrations are append-only.** Never edit or reorder an entry of `MIGRATIONS` in
   `src/storage/schema.rs` (two entries today; `APPLICATION_ID` marks our databases); schema
   changes are new entries. A new enum variant needs no migration (kind columns are free text).
9. **Raw text is the source of truth.** `normalized_hand_text` and `hand_text_sha256` define
   how re-imports are recognised; never change them.

## 4. How to work

- **One milestone at a time**, in strict sequential order. Never start the next milestone in the
  same turn as finishing the previous one; commit, report, and wait for the user.
- **Milestones 1–5 (v3.5 baseline):** extraction + verification via `tools/blueprint`.
- **Milestones 6–11 (v3.6 spec in `docs/SPEC_v3.6.md`):**
  - Follow the exact TDD order in `docs/SPEC_v3.6.md`: (1) add real hand fixture verbatim to
    `tests/common/mod.rs`; (2) write failing test; (3) update parser/validator/domain; (4) add
    near-miss refusal tests.
  - **Do NOT run `tools/blueprint extract` or `tools/blueprint check` in Milestones 6+**, as
    `docs/BLUEPRINT.md` is frozen at v3.5.
  - Run the gate (or the `milestone-gate` skill) at the end of every milestone:
    ```
    cargo fmt --check
    cargo clippy --locked --all-targets -- -D warnings
    cargo test --locked
    ```
    Confirm that 0 tests fail and the total passed-test count increases at each code milestone
    (v3.5 baseline = 104 tests).
- **Fixtures are read-only.** Never modify, reformat, regenerate or trim
  `tests/fixtures/**` or existing verbatim hands in `tests/common/mod.rs`. Opponent `Dealt to`
  lines end with a space; `.editorconfig` and `.gitattributes` keep them, and tests pin the
  exports' SHA-256 digests.
- **Commit** after each passing gate:
  `milestone <N>: <summary> (gate: <count> tests passing)`. Never force-push, rewrite history,
  or run destructive git commands.
- **Protected files:** Do not modify `AGENTS.md`, `.agents/`, `tools/`, or `Cargo.lock` unless
  the user asks. Do not modify `docs/` in Milestones 6–10; in Milestone 11, create and update
  `docs/invariants.md`, `docs/known_limitations.md`, `docs/CHANGELOG.md`, and the top note in
  `docs/BLUEPRINT.md` as specified in `docs/SPEC_v3.6.md` §8.

## 5. Repository layout

```text
poker_tracker/
├── AGENTS.md
├── Cargo.toml
├── Cargo.lock                       # shipped with the blueprint; never regenerate
├── .editorconfig  .gitattributes    # keep fixtures and the blueprint verbatim
├── docs/
│   ├── BLUEPRINT.md                 # v3.5 reference implementation (frozen)
│   ├── SPEC_v3.6.md                 # v3.6 implementation specification (M6–M11)
│   ├── invariants.md                # living invariants (created in M11)
│   ├── known_limitations.md         # living limitations (created in M11)
│   └── CHANGELOG.md                 # history (updated in M11)
├── tools/blueprint/                 # M1–M5 extract / check / list (std only)
├── .agents/
│   ├── rules/{ARCHITECTURE,TASKS}.md
│   └── skills/milestone-gate/SKILL.md
├── src/
│   ├── lib.rs
│   ├── main.rs
│   ├── domain/   {mod, money, card, canonical}.rs
│   ├── parser/   {mod, base, registry, ggpoker}.rs
│   ├── storage/  {mod, schema, repository}.rs
│   └── pipeline/ {mod, continuity, ingester}.rs
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

Exit codes: `0` ok · `1` fatal (including an unknown run id or a locked database) · `2` usage
(including a missing input directory) · `3` completed with rejected files or hands, or no
`.txt` files found.