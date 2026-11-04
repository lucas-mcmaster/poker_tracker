# AGENTS.md — `poker_tracker` Engineering Constitution

You are a senior Rust engineer on `poker_tracker`, a post-session poker tracker that ingests hand
history files into SQLite and derives statistics from them. Read this file in full before
changing anything, and follow it exactly. If it and any other instruction disagree, stop and ask
the user.

## 1. Sources of truth (in priority order)

1. **The current spec** in `docs/` (`SPEC_v<version>.md`, possibly with an amendment
   `SPEC_v<version>.<n>.md` that overrides it for the items it covers): tasks, contracts and
   expected numbers. Between versions there is none, and the user's request is the task (§4,
   "Work no spec names").
2. **`docs/stats.md`**: every statistic, position and money definition. On definitions it
   outranks the spec and the code.
3. **`docs/invariants.md`** and **`docs/known_limitations.md`**: the living rules and scope.
4. **The code** (`src/`, `tests/`).
5. **`.agents/rules/ARCHITECTURE.md`**: layers, module, storage, pipeline, CLI and report
   contracts, and how to extend the system.
6. This file: rules of engagement.

**History, never a specification:** `docs/history/` (finished specs and the frozen v3.5
`BLUEPRINT.md`; index in its `README.md`) and `docs/CHANGELOG.md` (which milestones append to, §4).
Do not read or search them unless the user or the spec points to a section. If two sources disagree,
stop and report the difference; do not pick one.

## 2. Toolchain

- Crate `poker_tracker` 0.5.0, `edition = "2024"`, `rust-version = "1.88"` (minimum); verified on
  rustc 1.97.0 (use the installed stable toolchain).
- **Always pass `--locked`** to `cargo build`, `test` and `clippy`. **Never pass
  `--ignore-rust-version`**: it lets Cargo pick dependencies that need a Rust newer than 1.88.
- Import the library as `use poker_tracker::...` in `src/main.rs` and `tests/`.
- Edition 2024 idioms:
  - `let`-chains (`if a && let Some(x) = y`) instead of nested `if`/`if let`;
  - `std::sync::LazyLock` for static regexes (never `lazy_static` or `once_cell`);
  - precise capturing (`+ use<'_>`) on a returned `impl Trait` that borrows its arguments;
  - `gen` is reserved: never an identifier.

  `HandParser` returns `Box<dyn Iterator>` (`ParsedBlocks`) so the trait stays object-safe; keep
  it so.
- `Cargo.toml` and `Cargo.lock` fix the dependencies. Never add, remove or upgrade one, bump the
  crate version, or regenerate the lockfile without the user's approval. Change nothing else in
  `Cargo.toml` unless the spec allows that exact change.

## 3. Non-negotiable invariants

1. **No floating point:** `f32`/`f64` are banned for money, chips, equities and statistics
   (`clippy::float_arithmetic = "deny"`). Use `rust_decimal::Decimal` in memory and `i64`
   micro-units (millionths: `1.00 = 1_000_000`) in SQLite via `decimal_to_micro_units`, which
   rejects rather than rounds. Read only integer columns from SQLite and compute ratios in Rust
   with `Decimal`; never divide in SQL. Equities are exact integer fractions. Convert integers
   with `try_from`; keep an `as` cast only where a comment says why it is lossless.
2. **No panics in `src/`** (`clippy::unwrap_used`/`expect_used = "deny"`): no `.unwrap()`,
   `.expect()`, `panic!`, `unreachable!` or `todo!`. The single exception is the
   `#[allow(clippy::expect_used)]` on `compile_static_regex` for constant regexes. Prefer
   `.get()` and slice patterns; index only where in bounds by construction. Errors are typed
   `thiserror` enums. Tests may `#![allow(clippy::unwrap_used, clippy::expect_used)]`.
3. **Site- and currency-independent core.** `src/domain/`, `src/parser/base.rs`,
   `src/parser/registry.rs`, `src/storage/`, `src/pipeline/`, `src/stats/` and `src/report/`
   contain no site text format or vocabulary. Site knowledge lives only in
   `src/parser/<site>.rs`; elsewhere the only site names are `Platform` variants and parser
   registration in `main.rs`.
4. **Currency is explicit.** Every hand carries a `CurrencyCode`. Never default a currency, infer
   one from a symbol, or add money across currencies (reports group by currency).
5. **Validation is mandatory.** A parser returns hands only after `validate_hand` succeeds, and
   storage re-validates. Never weaken, skip or special-case an invariant to make a hand pass.
6. **Format coverage.** Parse only line formats observed in real exports. An unknown line fails
   the hand loudly (`UnrecognizedLine`, `UnsupportedGame`, `UnsupportedAdjustment`,
   `IllegalAction`), never skipped. A failing hand is reported per hand, quarantined with its raw
   text, and never aborts an import.
7. **Stable identifiers.** Every `as_str()` value (`Platform`, `GameType`, `BettingLimit`,
   `ActionType`, `DeductionKind`, `AdjustmentKind`), `Street::code` and `hand_player_stats`
   column name is stored in SQLite: add values, never rename one. List each new variant in its
   `stored_identifier!` call.
8. **Migrations are append-only.** Never edit or reorder an entry of `MIGRATIONS`
   (`src/storage/schema.rs`); a schema change is a new entry. A new enum variant needs no
   migration (kind columns are free text).
9. **Raw text is the source of truth.** `normalized_hand_text` and `hand_text_sha256` define how
   re-imports are recognised; never change them.
10. **Derived data is a cache.** `hand_player_stats` is computed only from stored canonical
    hands, versioned by `STATS_VERSION` (outside `MIGRATIONS`), rebuilt when that changes, and
    never read by validation, ingestion or duplicate detection. Changing the definition of a
    stored column in `docs/stats.md` (position, counter, money or equity) bumps `STATS_VERSION`
    in the same commit; figures computed at report time (stat ratios, `bb_won`, `bb/100`,
    sessions) do not.

## 4. How to work

- **One milestone at a time**, in the spec's order; the last one done is the newest
  `milestone <N>:` commit. Never start the next milestone in the turn that finishes one.
- **The spec fixes the contract** (public names, stored identifiers, schema, CLI flags, output
  formats, expected numbers); **you design the internals** (module layout, private helpers,
  internal types, SQL) within this file. A spec's "e.g." is a suggestion.
- **Running a milestone:**
  1. Read its spec section, the living-doc sections it names, and `docs/stats.md` for anything
     statistical.
  2. Build it test-first (below), and update the living docs in the same commit (below).
  3. Run the gate (`milestone-gate` skill): `cargo fmt --check`,
     `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`. All must pass
     with 0 failures; the passed count rises at a code milestone and stays equal at a docs-only
     one.
  4. Do any scratch import or report its **Done when** asks for, then delete the scratch
     database.
  5. Commit `milestone <N>: <summary> (gate: <count> tests passing)`.
  6. Report: the gate block, the test count, **Deviations** (or "none"), the scratch-run numbers
     and `git show --stat HEAD`, written from the final commit (test names as in the code). Then
     wait for the user.
- **Test first.**
  - A new site format: add the real hand verbatim to `tests/common/mod.rs`; write a failing test;
    change the parser or validator; add near-miss refusal tests.
  - A refusal test asserts the error variant **and** a detail that identifies the rule, and is
    seen failing before the fix. A validator rule's near-miss test must fail when the rule is
    deleted (delete it, see the test fail, restore).
  - A derived statistic: pick fixtures that exercise it; write the expected values into a
    failing test, with a one-line derivation each; implement; add canonical near-miss tests (no
    site text).
  - Expected numbers come from an independent oracle (`tools/oracles/`). Never change one to
    match the code; if they differ, find which side departs from `docs/stats.md` and report it.
- **Deviations are disclosed.** Anything the milestone does not name (a refactor, an unrelated
  fix, another file layout) goes in its own commit and under **Deviations** in the report, with
  the reason. Never change behaviour silently. Editing an existing test needs explicit
  authorization from the spec or the user, and the report lists each such edit by test name.
- **Docs move with the code.** A milestone that adds or changes a rule updates `docs/invariants.md`
  and `docs/known_limitations.md` in the same commit, and the ARCHITECTURE sections its spec names
  for a changed contract. It adds one line per task to the version's section of `docs/CHANGELOG.md`
  (`## v<previous> → v<version>`); the version's last milestone also adds its row to the table with
  the final test count. Write each fact once, in the file that owns it. `docs/stats.md` changes only
  to fix a wrong or ambiguous definition: stop and report first.
- **Work no spec names:** propose a plan (files, public API, schema changes, tests), wait for
  approval, then implement against the same gate.
- **Fixtures are read-only:** never modify, reformat, regenerate or trim `tests/fixtures/**` or
  the verbatim hands in `tests/common/mod.rs`. Opponent `Dealt to` lines end with a space;
  `.editorconfig` and `.gitattributes` keep it, and tests pin the exports' SHA-256 digests.
- **Git:** never force-push, rewrite history, or run destructive commands.
- **Protected:** `AGENTS.md`, `.agents/`, `tools/`, `Cargo.lock` and `docs/history/` change only
  when the user or the current spec asks. Never run `tools/blueprint extract` or `check`: they would
  overwrite later work with the frozen v3.5 code.

## 5. Where things are

- `src/`: `domain/` (model, money, cards), `parser/` (`base.rs` validator, `registry.rs`,
  `ggpoker.rs`), `storage/`, `pipeline/`, `stats/`, `report/`, `main.rs` (CLI); duties in
  ARCHITECTURE §2.
- `tests/`: `common/mod.rs` (verbatim real hands, found by grepping `pub const <NAME>`, and the CAD
  parser helper); one file per area; `fixtures/ggpoker/` (real CAD exports:
  `NLHRed15_2026-08-21.txt`, 294 hands; `v3_6/*.txt`, 10 files, 989 hands); `fixtures/equity/`;
  `golden/` (byte-exact report CSVs).
- `tools/oracles/`: the independent Python oracles (`README.md`).

## 6. Running the tool

```
cargo run --release --locked -- <input-dir> --ggpoker-currency CAD [--db poker_tracker.sqlite]
cargo run --release --locked -- --delete-run <RUN-ID> [--db …]
cargo run --release --locked -- --rebuild-stats [--db …]
cargo run --release --locked -- --report <results|positions|sessions|stats> [--db …] [--stakes <sb>/<bb>[,…]] [--out <path>]
```

Exit codes:

- `0`: ok.
- `1`: fatal, including an unknown run id, a missing database for a command that needs one, a
  file that is not a `poker_tracker` database, and a locked database.
- `2`: usage, including a missing input directory.
- `3`: completed with rejected files or hands, or no `.txt` files found.

The full CLI contract is in ARCHITECTURE §7.
