//! Resilient directory ingestion pipeline, diagnostic reporting and cross-hand checks.

pub mod continuity;
pub mod ingester;

pub use continuity::{PlayerStacks, StackSnapshot, StackWarning, check_stack_continuity};
pub use ingester::{
    DEFAULT_BATCH_SIZE, DirectoryIngester, FailureStage, HandFailure, IngestionReport,
    PipelineError,
};
