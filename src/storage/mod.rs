//! SQLite persistence layer: `STRICT` tables, numbered migrations, per-hand savepoints,
//! per-hand provenance (every import run, file and line that contained a hand), the raw text of
//! every hand, and a quarantine for hands an import rejected.

pub mod repository;
pub mod schema;

pub use repository::{
    BatchInsertStats, DEFAULT_BUSY_TIMEOUT, DeletedImportRun, DerivedRebuild, HandSource,
    QuarantinedHand, RejectedHand, SqliteRepository, StorageError, hand_text_sha256,
    normalized_hand_text,
};
pub use schema::{APPLICATION_ID, MIGRATIONS, PRAGMA_TUNING, SCHEMA_VERSION};
