//! SQLite connection pragmas, database identity and numbered schema migrations.
//!
//! The schema is site- and currency-independent: `platform` and `currency` are columns, and
//! site vocabulary is stored as canonical kind strings (`as_str()` values) plus the site's
//! original label. Kind columns are deliberately *not* restricted to a fixed list: the Rust
//! enums are the vocabulary, so adding a variant (a new action, fee or adjustment kind) needs
//! no migration. Every money column is in micro-units (millionths, `1.00 = 1,000,000`) of the
//! hand's own `currency`.
//!
//! **Changing the schema:** never edit a released migration. Append a new SQL string to
//! [`MIGRATIONS`]; databases are upgraded in order, each step in its own transaction, and
//! `PRAGMA user_version` records how many have been applied.

/// `PRAGMA application_id` of every `poker_tracker` database (`"PKTR"` in ASCII). A file with
/// another id, or an id of `0` and tables that are not a v3.4 `poker_tracker` schema, is
/// refused before anything is written to it.
pub const APPLICATION_ID: i64 = 0x504B_5452;

/// Connection-level performance and integrity pragmas, applied only after the file has been
/// identified as a `poker_tracker` database (or as a new, empty one).
pub const PRAGMA_TUNING: &str = r"
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA foreign_keys = ON;
PRAGMA temp_store = MEMORY;
";

/// Migration 1: baseline schema (v3.4). Money columns were then in units of `0.0001`;
/// migration 2 rescales them to true micro-units.
const MIGRATION_0001_BASELINE: &str = r"
CREATE TABLE import_runs (
    id TEXT PRIMARY KEY,
    started_at TEXT NOT NULL,
    tool_version TEXT NOT NULL
) STRICT;

CREATE TABLE players (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    platform TEXT NOT NULL,
    screen_name TEXT NOT NULL,
    UNIQUE(platform, screen_name)
) STRICT;

CREATE TABLE hands (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    platform TEXT NOT NULL CHECK (length(platform) > 0),
    site_hand_id TEXT NOT NULL CHECK (length(site_hand_id) > 0),
    game_type TEXT NOT NULL CHECK (length(game_type) > 0),
    betting_limit TEXT NOT NULL CHECK (length(betting_limit) > 0),
    currency TEXT NOT NULL CHECK (length(currency) = 3 AND currency = upper(currency)),
    small_blind_micros INTEGER NOT NULL CHECK (small_blind_micros > 0),
    big_blind_micros INTEGER NOT NULL CHECK (big_blind_micros >= small_blind_micros),
    timestamp TEXT NOT NULL,
    utc_offset_seconds INTEGER CHECK (utc_offset_seconds BETWEEN -64800 AND 64800),
    table_name TEXT NOT NULL,
    max_seats INTEGER NOT NULL CHECK (max_seats BETWEEN 2 AND 10),
    button_seat INTEGER NOT NULL CHECK (button_seat >= 1 AND button_seat <= max_seats),
    run_count INTEGER NOT NULL CHECK (run_count >= 0),
    total_pot_micros INTEGER NOT NULL CHECK (total_pot_micros >= 0),
    rake_micros INTEGER NOT NULL CHECK (rake_micros >= 0),
    jackpot_micros INTEGER NOT NULL CHECK (jackpot_micros >= 0),
    other_deductions_micros INTEGER NOT NULL CHECK (other_deductions_micros >= 0),
    uncalled_returned_micros INTEGER NOT NULL CHECK (uncalled_returned_micros >= 0),
    import_run_id TEXT REFERENCES import_runs(id) ON DELETE SET NULL,
    source_file TEXT,
    source_line INTEGER CHECK (source_line IS NULL OR source_line >= 1),
    CHECK (rake_micros + jackpot_micros + other_deductions_micros <= total_pot_micros),
    UNIQUE(platform, site_hand_id)
) STRICT;

CREATE TABLE hand_players (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    hand_id INTEGER NOT NULL REFERENCES hands(id) ON DELETE CASCADE,
    player_id INTEGER NOT NULL REFERENCES players(id) ON DELETE RESTRICT,
    seat INTEGER NOT NULL CHECK (seat >= 1),
    starting_stack_micros INTEGER NOT NULL CHECK (starting_stack_micros > 0),
    hole_card_1 INTEGER CHECK (hole_card_1 IS NULL OR (hole_card_1 >= 0 AND hole_card_1 < 52)),
    hole_card_2 INTEGER CHECK (hole_card_2 IS NULL OR (hole_card_2 >= 0 AND hole_card_2 < 52)),
    is_hero INTEGER NOT NULL CHECK (is_hero IN (0, 1)),
    invested_micros INTEGER NOT NULL CHECK (invested_micros >= 0 AND invested_micros <= starting_stack_micros),
    won_micros INTEGER NOT NULL CHECK (won_micros >= 0),
    adjustments_micros INTEGER NOT NULL,
    net_micros INTEGER NOT NULL,
    CHECK (hole_card_2 IS NULL OR (hole_card_1 IS NOT NULL AND hole_card_1 != hole_card_2)),
    CHECK (is_hero = 0 OR hole_card_2 IS NOT NULL),
    CHECK (net_micros = won_micros - invested_micros + adjustments_micros),
    UNIQUE(hand_id, seat),
    UNIQUE(hand_id, player_id)
) STRICT;

CREATE TABLE actions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    hand_id INTEGER NOT NULL REFERENCES hands(id) ON DELETE CASCADE,
    player_id INTEGER NOT NULL REFERENCES players(id) ON DELETE RESTRICT,
    sequence INTEGER NOT NULL CHECK (sequence >= 0),
    street INTEGER NOT NULL CHECK (street >= 0 AND street <= 3),
    action_type TEXT NOT NULL CHECK (length(action_type) > 0),
    amount_micros INTEGER NOT NULL CHECK (amount_micros >= 0),
    street_total_after_micros INTEGER NOT NULL CHECK (street_total_after_micros >= 0),
    is_all_in INTEGER NOT NULL CHECK (is_all_in IN (0, 1)),
    UNIQUE(hand_id, sequence)
) STRICT;

CREATE TABLE board_cards (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    hand_id INTEGER NOT NULL REFERENCES hands(id) ON DELETE CASCADE,
    run_index INTEGER NOT NULL CHECK (run_index >= 0),
    flop_1 INTEGER CHECK (flop_1 IS NULL OR (flop_1 >= 0 AND flop_1 < 52)),
    flop_2 INTEGER CHECK (flop_2 IS NULL OR (flop_2 >= 0 AND flop_2 < 52)),
    flop_3 INTEGER CHECK (flop_3 IS NULL OR (flop_3 >= 0 AND flop_3 < 52)),
    turn INTEGER CHECK (turn IS NULL OR (turn >= 0 AND turn < 52)),
    river INTEGER CHECK (river IS NULL OR (river >= 0 AND river < 52)),
    UNIQUE(hand_id, run_index)
) STRICT;

CREATE TABLE pot_winners (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    hand_id INTEGER NOT NULL REFERENCES hands(id) ON DELETE CASCADE,
    player_id INTEGER NOT NULL REFERENCES players(id) ON DELETE RESTRICT,
    pot_sequence INTEGER NOT NULL CHECK (pot_sequence >= 0),
    run_index INTEGER NOT NULL CHECK (run_index >= 0),
    amount_micros INTEGER NOT NULL CHECK (amount_micros > 0),
    UNIQUE(hand_id, pot_sequence)
) STRICT;

CREATE TABLE pot_deductions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    hand_id INTEGER NOT NULL REFERENCES hands(id) ON DELETE CASCADE,
    deduction_sequence INTEGER NOT NULL CHECK (deduction_sequence >= 0),
    kind TEXT NOT NULL CHECK (length(kind) > 0),
    site_label TEXT NOT NULL,
    amount_micros INTEGER NOT NULL CHECK (amount_micros >= 0),
    UNIQUE(hand_id, deduction_sequence)
) STRICT;

CREATE TABLE player_adjustments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    hand_id INTEGER NOT NULL REFERENCES hands(id) ON DELETE CASCADE,
    player_id INTEGER NOT NULL REFERENCES players(id) ON DELETE RESTRICT,
    adjustment_sequence INTEGER NOT NULL CHECK (adjustment_sequence >= 0),
    kind TEXT NOT NULL CHECK (length(kind) > 0),
    street INTEGER NOT NULL CHECK (street >= 0 AND street <= 3),
    amount_micros INTEGER NOT NULL CHECK (amount_micros != 0),
    UNIQUE(hand_id, adjustment_sequence)
) STRICT;

CREATE INDEX idx_hands_timestamp ON hands(timestamp);
CREATE INDEX idx_hands_currency_platform ON hands(currency, platform);
CREATE INDEX idx_hands_import_run ON hands(import_run_id);
CREATE INDEX idx_hand_players_player ON hand_players(player_id, hand_id);
CREATE INDEX idx_actions_hand_street ON actions(hand_id, street, sequence);
CREATE INDEX idx_pot_winners_hand ON pot_winners(hand_id, pot_sequence);
";

/// Migration 2 (v3.5):
///
/// - every `*_micros` column becomes true micro-units (`1.00 = 1,000,000`, previously `10,000`);
/// - `hands.raw_text` keeps the hand exactly as imported, and `hands.raw_sha256` fingerprints
///   it (SHA-256 of the text with line endings and trailing spaces normalised), so a re-import
///   with different content is detected and old hands can be re-parsed later;
/// - `hand_sightings` records every import run that contained a hand, so undoing one run never
///   deletes a hand that another run also imported;
/// - `quarantined_hands` keeps the raw text of every hand an import rejected, per run, and the
///   `quarantine_pending` view lists the entries whose text is not (or no longer) stored;
/// - the index duplicating `pot_winners`' unique `(hand_id, pot_sequence)` is dropped.
const MIGRATION_0002_MICRO_UNITS_RAW_TEXT_SIGHTINGS_QUARANTINE: &str = r"
UPDATE hands SET
    small_blind_micros = small_blind_micros * 100,
    big_blind_micros = big_blind_micros * 100,
    total_pot_micros = total_pot_micros * 100,
    rake_micros = rake_micros * 100,
    jackpot_micros = jackpot_micros * 100,
    other_deductions_micros = other_deductions_micros * 100,
    uncalled_returned_micros = uncalled_returned_micros * 100;
UPDATE hand_players SET
    starting_stack_micros = starting_stack_micros * 100,
    invested_micros = invested_micros * 100,
    won_micros = won_micros * 100,
    adjustments_micros = adjustments_micros * 100,
    net_micros = net_micros * 100;
UPDATE actions SET
    amount_micros = amount_micros * 100,
    street_total_after_micros = street_total_after_micros * 100;
UPDATE pot_winners SET amount_micros = amount_micros * 100;
UPDATE pot_deductions SET amount_micros = amount_micros * 100;
UPDATE player_adjustments SET amount_micros = amount_micros * 100;

ALTER TABLE hands ADD COLUMN raw_text TEXT;
ALTER TABLE hands ADD COLUMN raw_sha256 TEXT
    CHECK (raw_sha256 IS NULL OR (length(raw_sha256) = 64 AND raw_text IS NOT NULL));

CREATE TABLE hand_sightings (
    hand_id INTEGER NOT NULL REFERENCES hands(id) ON DELETE CASCADE,
    import_run_id TEXT NOT NULL REFERENCES import_runs(id) ON DELETE CASCADE,
    source_file TEXT NOT NULL,
    source_line INTEGER NOT NULL CHECK (source_line >= 1),
    PRIMARY KEY (hand_id, import_run_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX idx_hand_sightings_run ON hand_sightings(import_run_id);
INSERT INTO hand_sightings (hand_id, import_run_id, source_file, source_line)
    SELECT id, import_run_id, source_file, source_line FROM hands
    WHERE import_run_id IS NOT NULL AND source_file IS NOT NULL AND source_line IS NOT NULL;

CREATE TABLE quarantined_hands (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    import_run_id TEXT NOT NULL REFERENCES import_runs(id) ON DELETE CASCADE,
    raw_sha256 TEXT NOT NULL CHECK (length(raw_sha256) = 64),
    raw_text TEXT NOT NULL CHECK (length(raw_text) > 0),
    parser TEXT NOT NULL,
    site_hand_id TEXT,
    stage TEXT NOT NULL CHECK (length(stage) > 0),
    error TEXT NOT NULL,
    source_file TEXT NOT NULL,
    source_line INTEGER NOT NULL CHECK (source_line >= 1),
    UNIQUE (import_run_id, raw_sha256)
) STRICT;
CREATE INDEX idx_quarantined_hands_sha ON quarantined_hands(raw_sha256);
CREATE INDEX idx_hands_raw_sha256 ON hands(raw_sha256);
CREATE VIEW quarantine_pending AS
    SELECT q.* FROM quarantined_hands q
    WHERE NOT EXISTS (SELECT 1 FROM hands h WHERE h.raw_sha256 = q.raw_sha256);

DROP INDEX idx_pot_winners_hand;
";

/// All migrations, in order. Migration *n* (1-based) upgrades `user_version` *n − 1* to *n*.
/// Append only; never edit or reorder a released entry.
pub const MIGRATIONS: &[&str] = &[
    MIGRATION_0001_BASELINE,
    MIGRATION_0002_MICRO_UNITS_RAW_TEXT_SIGHTINGS_QUARANTINE,
];

/// Schema version this build creates and expects (`PRAGMA user_version`).
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;
