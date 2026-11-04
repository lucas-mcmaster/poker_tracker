//! Deterministic directory ingestion pipeline with per-file parser detection, per-hand failure
//! diagnostics, a quarantine for rejected hands, and stack-continuity warnings.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::domain::canonical::CanonicalHand;
use crate::parser::base::ParseError;
use crate::parser::registry::ParserRegistry;
use crate::pipeline::continuity::{StackSnapshot, StackWarning, check_stack_continuity};
use crate::storage::repository::{HandSource, QuarantinedHand, SqliteRepository, StorageError};

/// Default number of hands buffered before committing a SQLite transaction.
pub const DEFAULT_BATCH_SIZE: usize = 1_000;

/// Fatal pipeline-level errors. Carries the report accumulated before the failure; hands from
/// the failed batch are not counted as ingested.
#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("storage transaction failed: {source}")]
    Storage {
        source: StorageError,
        partial_report: Box<IngestionReport>,
    },
}

/// Stage at which a hand was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureStage {
    Parse(ParseError),
    /// Rejected by the storage layer (message of the underlying [`StorageError`]).
    Storage(String),
}

impl FailureStage {
    /// Stage name stored with a quarantined hand (`"parse"` or `"storage"`).
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Parse(_) => "parse",
            Self::Storage(_) => "storage",
        }
    }
}

impl fmt::Display for FailureStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "{e}"),
            Self::Storage(msg) => write!(f, "rejected by storage: {msg}"),
        }
    }
}

/// Diagnostic record for a single hand that could not be ingested.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandFailure {
    pub file_path: PathBuf,
    /// Parser that handled the file (see [`crate::parser::HandParser::describe`]).
    pub parser: String,
    /// 1-based line in `file_path` where the hand block starts.
    pub block_start_line: usize,
    /// Site hand id, if the header line could be read.
    pub hand_id: Option<String>,
    pub stage: FailureStage,
}

impl HandFailure {
    /// Best line number in the source file for this failure.
    #[must_use]
    pub fn file_line(&self) -> usize {
        let relative = match &self.stage {
            FailureStage::Parse(
                ParseError::IncompleteStreet { line, .. }
                | ParseError::IllegalAction { line, .. }
                | ParseError::UnrecognizedLine { line, .. }
                | ParseError::UnsupportedAdjustment { line, .. },
            ) => *line,
            _ => 0,
        };
        self.block_start_line + relative.saturating_sub(1)
    }
}

impl fmt::Display for HandFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let id = self.hand_id.as_deref().unwrap_or("<unknown id>");
        write!(
            f,
            "{}:{} [{}] hand {id}: {}",
            self.file_path.display(),
            self.file_line(),
            self.parser,
            self.stage
        )
    }
}

/// Telemetry report produced by a directory ingestion run.
#[derive(Debug, Clone)]
pub struct IngestionReport {
    /// Import run id; recorded for every hand this run contained (see
    /// [`SqliteRepository::delete_import_run`]).
    pub batch_run_id: Uuid,
    /// When the run started (stored in `import_runs.started_at`).
    pub started_at: DateTime<Utc>,
    pub files_processed: usize,
    /// Number of files handled by each parser (keyed by [`crate::parser::HandParser::describe`]).
    pub files_by_parser: BTreeMap<String, usize>,
    pub hands_ingested: usize,
    pub duplicates_skipped: usize,
    pub hands_failed: usize,
    /// Rejected hands stored with their raw text in `quarantined_hands` by this run.
    pub hands_quarantined: usize,
    /// Unreadable files, files no single parser recognized, and an input with no files.
    pub file_errors: Vec<String>,
    pub hand_failures: Vec<HandFailure>,
    /// Stacks that shrank between consecutive hands at a table (warnings, not failures).
    pub stack_warnings: Vec<StackWarning>,
}

impl IngestionReport {
    /// Creates a new ingestion report with the current time.
    #[must_use]
    pub fn new() -> Self {
        Self::new_at(Utc::now())
    }

    /// Creates a new ingestion report with the given start time.
    #[must_use]
    pub fn new_at(started_at: DateTime<Utc>) -> Self {
        Self {
            batch_run_id: Uuid::now_v7(),
            started_at,
            files_processed: 0,
            files_by_parser: BTreeMap::new(),
            hands_ingested: 0,
            duplicates_skipped: 0,
            hands_failed: 0,
            hands_quarantined: 0,
            file_errors: Vec::new(),
            hand_failures: Vec::new(),
            stack_warnings: Vec::new(),
        }
    }

    fn record_failure(&mut self, failure: HandFailure) {
        self.hands_failed += 1;
        self.hand_failures.push(failure);
    }

    /// `true` if every file was recognized and every hand was stored or was a duplicate.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.file_errors.is_empty() && self.hands_failed == 0
    }
}

impl Default for IngestionReport {
    fn default() -> Self {
        Self::new()
    }
}

/// Where a buffered hand came from, for diagnostics, provenance and quarantine.
#[derive(Debug, Clone)]
struct Origin {
    file_path: PathBuf,
    parser: String,
    block_start_line: usize,
    hand_id: String,
    raw_text: String,
}

/// Hands and rejections waiting to be written in one batch.
#[derive(Default)]
struct Pending {
    hands: Vec<CanonicalHand>,
    origins: Vec<Origin>,
    quarantine: Vec<QuarantinedHand>,
}

impl Pending {
    fn len(&self) -> usize {
        self.hands.len() + self.quarantine.len()
    }
}

/// Coordinates recursive file discovery, per-file parser selection, streaming parsing, and
/// batched SQLite persistence.
#[derive(Debug, Clone, Copy)]
pub struct DirectoryIngester {
    batch_size: usize,
}

impl Default for DirectoryIngester {
    fn default() -> Self {
        Self {
            batch_size: DEFAULT_BATCH_SIZE,
        }
    }
}

fn quarantine_entry(report: &IngestionReport, failure: &HandFailure, raw: &str) -> QuarantinedHand {
    QuarantinedHand {
        import_run_id: report.batch_run_id,
        parser: failure.parser.clone(),
        file_path: failure.file_path.display().to_string(),
        start_line: failure.block_start_line,
        site_hand_id: failure.hand_id.clone(),
        stage: failure.stage.as_str().to_string(),
        error: failure.stage.to_string(),
        raw_text: raw.to_string(),
    }
}

impl DirectoryIngester {
    /// Creates a [`DirectoryIngester`] with a custom transaction batch size (minimum 1).
    #[must_use]
    pub const fn with_batch_size(batch_size: usize) -> Self {
        let effective = if batch_size == 0 { 1 } else { batch_size };
        Self {
            batch_size: effective,
        }
    }

    fn flush(
        repo: &mut SqliteRepository,
        pending: &mut Pending,
        report: &mut IngestionReport,
    ) -> Result<(), PipelineError> {
        if pending.len() == 0 {
            return Ok(());
        }
        let fail = |source: StorageError, report: &IngestionReport| PipelineError::Storage {
            source,
            partial_report: Box::new(report.clone()),
        };
        let sources: Vec<HandSource> = pending
            .origins
            .iter()
            .map(|o| HandSource {
                import_run_id: report.batch_run_id,
                file_path: o.file_path.display().to_string(),
                start_line: o.block_start_line,
                raw_text: o.raw_text.clone(),
            })
            .collect();
        let stats = repo
            .insert_sourced_hands_batch_at(&pending.hands, &sources, Some(report.started_at))
            .map_err(|e| fail(e, report))?;
        report.hands_ingested += stats.hands_inserted;
        report.duplicates_skipped += stats.duplicates_skipped;
        for rejected in stats.rejected {
            let origin = pending.origins.get(rejected.batch_index);
            let failure = HandFailure {
                file_path: origin.map(|o| o.file_path.clone()).unwrap_or_default(),
                parser: origin.map(|o| o.parser.clone()).unwrap_or_default(),
                block_start_line: origin.map_or(0, |o| o.block_start_line),
                hand_id: Some(origin.map_or(rejected.site_hand_id, |o| o.hand_id.clone())),
                stage: FailureStage::Storage(rejected.error.to_string()),
            };
            if let Some(o) = origin {
                let entry = quarantine_entry(report, &failure, &o.raw_text);
                pending.quarantine.push(entry);
            }
            report.record_failure(failure);
        }
        report.hands_quarantined += repo
            .quarantine_hands_at(&pending.quarantine, Some(report.started_at))
            .map_err(|e| fail(e, report))?;
        pending.hands.clear();
        pending.origins.clear();
        pending.quarantine.clear();
        Ok(())
    }

    /// Recursively discovers `.txt` files under `root_dir` (following symbolic links) and
    /// processes them in sorted path order. For each file, the registry selects the one parser
    /// that recognizes it; every hand block is parsed, valid hands are committed in batches, and
    /// rejected hands are quarantined with their raw text. Paths are recorded as absolute paths.
    ///
    /// # Errors
    /// Returns [`PipelineError`] (with the partial report) if a SQLite transaction fails.
    /// Unreadable or unrecognized files, an input without `.txt` files, and per-hand
    /// parse/storage rejections are recorded in the report, as are stack-continuity warnings.
    pub fn ingest_directory<P: AsRef<Path>>(
        &self,
        root_dir: P,
        registry: &ParserRegistry,
        repo: &mut SqliteRepository,
    ) -> Result<IngestionReport, PipelineError> {
        self.ingest_directory_at(root_dir, registry, repo, Utc::now())
    }

    /// Recursively discovers `.txt` files under `root_dir` (following symbolic links) and
    /// processes them in sorted path order, recording `started_at` as the import run start time.
    ///
    /// # Errors
    /// As [`Self::ingest_directory`].
    pub fn ingest_directory_at<P: AsRef<Path>>(
        &self,
        root_dir: P,
        registry: &ParserRegistry,
        repo: &mut SqliteRepository,
        started_at: DateTime<Utc>,
    ) -> Result<IngestionReport, PipelineError> {
        let mut report = IngestionReport::new_at(started_at);
        let root = std::path::absolute(root_dir.as_ref())
            .unwrap_or_else(|_| root_dir.as_ref().to_path_buf());
        let mut txt_files: Vec<PathBuf> = Vec::new();

        for entry_res in WalkDir::new(&root).follow_links(true) {
            match entry_res {
                Ok(entry) => {
                    let path = entry.path();
                    if entry.file_type().is_file()
                        && let Some(ext) = path.extension()
                        && ext.eq_ignore_ascii_case("txt")
                    {
                        txt_files.push(path.to_path_buf());
                    }
                }
                Err(walk_err) => report.file_errors.push(walk_err.to_string()),
            }
        }
        txt_files.sort();
        if txt_files.is_empty() && report.file_errors.is_empty() {
            report.file_errors.push(format!(
                "{}: no hand-history files (*.txt) found",
                root.display()
            ));
        }

        let mut pending = Pending::default();
        for file_path in txt_files {
            let raw_content = match fs::read_to_string(&file_path) {
                Ok(content) => content,
                Err(io_err) => {
                    report
                        .file_errors
                        .push(format!("{}: {io_err}", file_path.display()));
                    continue;
                }
            };
            let parser = match registry.detect(&raw_content) {
                Ok(parser) => parser,
                Err(detect_err) => {
                    report
                        .file_errors
                        .push(format!("{}: {detect_err}", file_path.display()));
                    continue;
                }
            };
            let parser_name = parser.describe();
            report.files_processed += 1;
            *report
                .files_by_parser
                .entry(parser_name.clone())
                .or_insert(0) += 1;

            let mut snapshots: Vec<StackSnapshot> = Vec::new();
            let mut unparsed_lines: Vec<usize> = Vec::new();
            for (block, result) in parser.parse_session(&raw_content) {
                match result {
                    Ok(hand) => {
                        snapshots.extend(StackSnapshot::of(&hand, block.start_line));
                        pending.origins.push(Origin {
                            file_path: file_path.clone(),
                            parser: parser_name.clone(),
                            block_start_line: block.start_line,
                            hand_id: hand.header.site_hand_id.clone(),
                            raw_text: block.text.to_string(),
                        });
                        pending.hands.push(hand);
                    }
                    Err(parse_err) => {
                        unparsed_lines.push(block.start_line);
                        let failure = HandFailure {
                            file_path: file_path.clone(),
                            parser: parser_name.clone(),
                            block_start_line: block.start_line,
                            hand_id: parser.peek_hand_id(block.text),
                            stage: FailureStage::Parse(parse_err),
                        };
                        pending
                            .quarantine
                            .push(quarantine_entry(&report, &failure, block.text));
                        report.record_failure(failure);
                    }
                }
                if pending.len() >= self.batch_size {
                    Self::flush(repo, &mut pending, &mut report)?;
                }
            }
            report.stack_warnings.extend(check_stack_continuity(
                &file_path,
                &mut snapshots,
                &unparsed_lines,
            ));
        }
        Self::flush(repo, &mut pending, &mut report)?;
        Ok(report)
    }
}
