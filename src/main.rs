//! CLI entrypoint for `poker_tracker`.
//!
//! - Import: `poker_tracker <input-dir> --ggpoker-currency <CODE> [--db <path>]`
//! - Undo an import run: `poker_tracker --delete-run <RUN-ID> [--db <path>]`
//!
//! Each supported site is enabled by its own option (given at most once). Sites whose exports
//! do not state the account currency (GGPoker) take it as the option value, in any letter case.
//! The parser for each file is detected automatically.
//!
//! Exit codes: `0` success (and `--help`), `1` fatal error, `2` usage error (including a missing
//! input directory), `3` import completed but some files or hands were rejected.

use std::env;
use std::path::Path;
use std::process::ExitCode;

use poker_tracker::domain::CurrencyCode;
use poker_tracker::parser::{GgPokerParser, ParserRegistry};
use poker_tracker::pipeline::{DirectoryIngester, IngestionReport, PipelineError};
use poker_tracker::storage::SqliteRepository;
use uuid::Uuid;

const USAGE: &str = "\
Usage:
  poker_tracker <input-dir> [site options] [--db <path>]   Import hand histories
  poker_tracker --delete-run <RUN-ID> [--db <path>]        Undo one import run
  poker_tracker --help                                     Show this help

Site options (enable at least one when importing; each at most once):
  --ggpoker-currency <CODE>   Parse GGPoker exports; amounts are in CODE (e.g. CAD, USD).
                              GGPoker prints `$` for every account currency, so it must be given.

Other options:
  --db <path>                 SQLite database (default: poker_tracker.sqlite)
  --delete-run <RUN-ID>       Undo an import (the Batch Run ID printed by that import). Hands
                              that another import also contained are kept.

Exit codes: 0 success, 1 fatal error, 2 usage error, 3 some files or hands were rejected.";

enum Command {
    Import {
        input_dir: String,
        registry: ParserRegistry,
    },
    DeleteRun(Uuid),
}

struct CliArgs {
    db_path: String,
    command: Command,
}

enum Parsed {
    Run(CliArgs),
    Help,
}

fn take_value(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
    seen: &mut Vec<String>,
) -> Result<String, String> {
    if seen.iter().any(|f| f == flag) {
        return Err(format!("{flag} given more than once"));
    }
    seen.push(flag.to_string());
    args.next()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Parsed, String> {
    let mut input_dir = None;
    let mut db_path = "poker_tracker.sqlite".to_string();
    let mut registry = ParserRegistry::new();
    let mut delete_run = None;
    let mut seen: Vec<String> = Vec::new();
    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => db_path = take_value(&mut args, "--db", &mut seen)?,
            "--ggpoker-currency" => {
                let raw = take_value(&mut args, "--ggpoker-currency", &mut seen)?;
                let currency: CurrencyCode = raw
                    .to_ascii_uppercase()
                    .parse()
                    .map_err(|e| format!("{e}"))?;
                registry.register(GgPokerParser::new(currency));
            }
            "--delete-run" => {
                let raw = take_value(&mut args, "--delete-run", &mut seen)?;
                let id = Uuid::parse_str(&raw).map_err(|e| format!("invalid run id: {e}"))?;
                delete_run = Some(id);
            }
            "-h" | "--help" => return Ok(Parsed::Help),
            flag if flag.starts_with("--") => return Err(format!("unknown option '{flag}'")),
            _ if input_dir.is_none() => input_dir = Some(arg),
            _ => return Err(format!("unexpected argument '{arg}'")),
        }
    }

    let command = match (delete_run, input_dir) {
        (Some(run), None) if registry.is_empty() => Command::DeleteRun(run),
        (Some(_), _) => {
            return Err("--delete-run cannot be combined with an import".to_string());
        }
        (None, Some(input_dir)) => {
            if registry.is_empty() {
                return Err("no site enabled; pass e.g. --ggpoker-currency CAD".to_string());
            }
            if !Path::new(&input_dir).is_dir() {
                return Err(format!(
                    "input directory '{input_dir}' does not exist or is not a directory"
                ));
            }
            Command::Import {
                input_dir,
                registry,
            }
        }
        (None, None) => return Err("missing <input-dir>".to_string()),
    };
    Ok(Parsed::Run(CliArgs { db_path, command }))
}

fn print_report(report: &IngestionReport) {
    println!("=== Poker Tracker Ingestion Summary ===");
    println!("Batch Run ID       : {}", report.batch_run_id);
    println!("Files Processed    : {}", report.files_processed);
    for (parser, files) in &report.files_by_parser {
        println!("  {parser:<17}: {files} file(s)");
    }
    println!("Hands Ingested     : {}", report.hands_ingested);
    println!("Duplicates Skipped : {}", report.duplicates_skipped);
    println!("Hands Failed       : {}", report.hands_failed);
    println!("Hands Quarantined  : {}", report.hands_quarantined);
    println!("File Errors        : {}", report.file_errors.len());
    println!("Stack Warnings     : {}", report.stack_warnings.len());
    for file_err in &report.file_errors {
        eprintln!("File error: {file_err}");
    }
    for failure in &report.hand_failures {
        eprintln!("Hand failure: {failure}");
    }
    for warning in &report.stack_warnings {
        eprintln!("Stack warning: {warning}");
    }
}

fn delete_run(db_path: &str, run: Uuid) -> ExitCode {
    // Never create a database just to delete from it.
    let mut repo = match SqliteRepository::open_existing(db_path) {
        Ok(r) => r,
        Err(err) => {
            eprintln!("Failed to open SQLite database at '{db_path}': {err}");
            return ExitCode::from(1);
        }
    };
    match repo.delete_import_run(run) {
        Ok(deleted) => {
            println!(
                "Deleted {} hand(s) from import run {run}; kept {} hand(s) that another import \
                 also contained; removed {} quarantined hand(s)",
                deleted.hands_deleted, deleted.hands_kept, deleted.quarantined_deleted
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Failed to delete import run {run}: {err}");
            ExitCode::from(1)
        }
    }
}

fn import(db_path: &str, input_dir: &str, registry: &ParserRegistry) -> ExitCode {
    let mut repo = match SqliteRepository::open(db_path) {
        Ok(r) => r,
        Err(err) => {
            eprintln!("Failed to open SQLite database at '{db_path}': {err}");
            return ExitCode::from(1);
        }
    };
    match DirectoryIngester::default().ingest_directory(input_dir, registry, &mut repo) {
        Ok(report) => {
            print_report(&report);
            if report.is_complete() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(3)
            }
        }
        Err(PipelineError::Storage {
            source,
            partial_report,
        }) => {
            print_report(&partial_report);
            eprintln!("Fatal storage error (the batch in progress was not saved): {source}");
            ExitCode::from(1)
        }
    }
}

fn main() -> ExitCode {
    let args = match parse_args(env::args().skip(1)) {
        Ok(Parsed::Run(a)) => a,
        Ok(Parsed::Help) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(msg) => {
            eprintln!("Error: {msg}\n");
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match &args.command {
        Command::DeleteRun(run) => delete_run(&args.db_path, *run),
        Command::Import {
            input_dir,
            registry,
        } => import(&args.db_path, input_dir, registry),
    }
}
