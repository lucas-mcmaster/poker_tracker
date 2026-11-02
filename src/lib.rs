//! `poker_tracker` core library.
//!
//! Site-independent domain model and validator, pluggable site parsers selected per file,
//! transactional SQLite storage, and a deterministic batch ingestion pipeline.

pub mod domain;
pub mod parser;
pub mod pipeline;
pub mod storage;
