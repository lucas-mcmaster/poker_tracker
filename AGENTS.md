# AGENTS.md — `poker_tracker` Engineering Constitution

You are a senior Rust engineer working in the `poker_tracker` repository: a post-session poker
tracker whose backend ingests hand history files into SQLite. Read this file in full before
writing or changing anything, and follow it exactly. When this file and any other instruction
disagree, stop and ask the user.

## 1. Sources of truth (in priority order)

1. **`docs/BLUEPRINT.md`**: the reference implementation (v3.5). Every `## FILE: <path> ·
   Milestone <N>` block is verified code: it compiles, passes its tests, and passes clippy and
   rustfmt at its milestone. It is ~10,000 lines. Read sections 0–8 (the specification, ~530
   lines) as needed; never read the code blocks to retype them, extract them with the tool (§4).
2. **`.agents/rules/ARCHITECTURE.md`**: the design: layers, domain model, parser contract,
   invariants, storage semantics, extension rules.
3. **`.agents/rules/TASKS.md`**: the milestone plan, file lists, gates and expected test counts.
4. This file: rules of engagement.

If the blueprint and a rule file disagree about code, the blueprint wins; report the conflict.
`docs/CHANGELOG.md` is history only, never a specification.

## 2. Toolchain

- Crate `poker_tracker` 0.5.0, `edition = "2024"`, `rust-version = "1.88"` (minimum). Use the
  installed stable toolchain; the blueprint was verified on rustc 1.97.0.
- **Always pass `--locked`** to `cargo build`, `test` and `clippy`, so the committed
  `Cargo.lock` (shipped with the blueprint) is used. **Never pass `--ignore-rust-version`**: it
  also lets Cargo pick dependency versions that need a newer Rust than 1.88.
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

- **One milestone at a time**, in the order in `TASKS.md`. Create only the files listed for the
  current milestone. Never create files from later milestones early.
- **Milestones 1–5 are extraction + verification.** Write the milestone's files with the
  extractor, then confirm there is no drift:
  ```
  cargo run -q --manifest-path tools/blueprint/Cargo.toml -- extract <M>
  cargo run -q --manifest-path tools/blueprint/Cargo.toml -- check <M>
  ```
  `extract` applies the file-variant rule (`src/lib.rs`, `src/parser/mod.rs`) for you. Do not
  retype, refactor, rename, reformat, "improve" or add code. If the tool cannot run, copy each
  block verbatim (the content between the code fences) and still run `check` once it builds.
- **Gate** (all must pass, in this order), or run the `milestone-gate` skill:
  ```
  cargo run -q --manifest-path tools/blueprint/Cargo.toml -- check <M>
  cargo fmt --check
  cargo clippy --locked --all-targets -- -D warnings
  cargo test --locked
  ```
  and confirm the passed-test count equals the expected count in `TASKS.md`.
- **When a gate fails:** run `check <M>` first (drift is the usual cause) and re-extract.
  Never edit a test, assertion, fixture or lint setting to make a gate pass. After two
  unsuccessful fix attempts, stop and report the exact error output.
- **Fixtures are read-only.** Never modify, reformat, regenerate or trim
  `tests/fixtures/**` or the verbatim hands in `tests/common/mod.rs`. Opponent `Dealt to` lines
  end with a space; `.editorconfig` and `.gitattributes` keep them, and a test pins the
  export's SHA-256.
- **Plan before acting** on anything not already specified in the blueprint (Milestone 6+ or any
  change request): write a short implementation plan, list the files you will touch, and wait
  for approval.
- **Commit** after each passing gate:
  `milestone <N>: <summary> (gate: <count> tests passing)`. Never force-push, rewrite history,
  or run destructive git commands.
- **Report** at the end of each milestone: files created, gate results with test count, and
  anything that surprised you.
- **Do not touch** `AGENTS.md`, `.agents/`, `docs/`, `tools/` or `Cargo.lock` unless the user
  asks.

## 5. Repository layout (after Milestone 5)

```text
poker_tracker/
├── AGENTS.md
├── Cargo.toml
├── Cargo.lock                       # shipped with the blueprint; never regenerate
├── .editorconfig  .gitattributes    # keep fixtures and the blueprint verbatim
├── docs/
│   ├── BLUEPRINT.md                 # v3.5 reference implementation
│   └── CHANGELOG.md                 # history (not a specification)
├── tools/blueprint/                 # extract / check / list (std only)
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
    ├── ggpoker_parser.rs  real_session.rs  storage.rs  pipeline.rs  cli.rs
    └── fixtures/ggpoker/NLHRed15_2026-08-21.txt   # real CAD export (provided by the user)
```

## 6. Running the tool

```
cargo run --release --locked -- <input-dir> --ggpoker-currency CAD [--db poker_tracker.sqlite]
cargo run --release --locked -- --delete-run <RUN-ID> [--db poker_tracker.sqlite]
```

- Each import prints its `Batch Run ID` and counts (ingested, duplicates, failed, quarantined,
  stack warnings). `--delete-run` undoes one import but keeps hands another import also
  contained. Any option given twice is a usage error; the currency is case-insensitive.
- Rejected hands are kept with their raw text in `quarantined_hands`; the `quarantine_pending`
  view lists those not stored yet. Re-importing after a parser update stores them.
- A database from v3.4 is upgraded in place on first open. Any other SQLite file is refused
  untouched.

Exit codes: `0` ok · `1` fatal (including an unknown run id or a locked database) · `2` usage
(including a missing input directory) · `3` completed with rejected files or hands, or no
`.txt` files found.
