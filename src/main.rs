//! CLI entrypoint for `poker_tracker`.
//!
//! - Import: `poker_tracker <input-dir> --ggpoker-currency <CODE> [--db <path>]`
//! - Undo an import run: `poker_tracker --delete-run <RUN-ID> [--db <path>]`
//! - Rebuild statistics: `poker_tracker --rebuild-stats [--db <path>]`
//! - Report: `poker_tracker --report <KIND> [--db <path>] [--stakes <LIST>] [--out <path>]`
//!
//! Each supported site is enabled by its own option (given at most once). Sites whose exports
//! do not state the account currency (GGPoker) take it as the option value, in any letter case.
//! The parser for each file is detected automatically.
//!
//! Exit codes: `0` success (and `--help`), `1` fatal error, `2` usage error (including a missing
//! input directory), `3` import completed but some files or hands were rejected.

use std::env;
use std::io::Write as _;
use std::path::Path;
use std::process::ExitCode;

use poker_tracker::domain::CurrencyCode;
use poker_tracker::parser::{GgPokerParser, ParserRegistry};
use poker_tracker::pipeline::{DirectoryIngester, IngestionReport, PipelineError};
use poker_tracker::report::{ReportKind, ReportOptions, Stakes, write_report};
use poker_tracker::storage::SqliteRepository;
use uuid::Uuid;

const USAGE: &str = "\
Usage:
  poker_tracker <input-dir> [site options] [--db <path>]   Import hand histories
  poker_tracker --delete-run <RUN-ID> [--db <path>]        Undo one import run
  poker_tracker --rebuild-stats [--db <path>]              Rebuild statistics cache
  poker_tracker --report <KIND> [--db <path>] [--stakes <LIST>] [--out <PATH>]
                                                           Generate a CSV report
  poker_tracker --help                                     Show this help

Report kinds: results, positions, sessions, stats

Site options (enable at least one when importing; each at most once):
  --ggpoker-currency <CODE>   Parse GGPoker exports; amounts are in CODE (e.g. CAD, USD).
                              GGPoker prints `$` for every account currency, so it must be given.

Other options:
  --db <path>                 SQLite database (default: poker_tracker.sqlite)
  --delete-run <RUN-ID>       Undo an import (the Batch Run ID printed by that import). Hands
                              that another import also contained are kept.
  --rebuild-stats             Rebuild the derived statistics cache for all hands
  --report <KIND>             Report kind: results, positions, sessions, or stats
  --stakes <LIST>             Comma-separated list of stakes to include (e.g. 0.05/0.1,0.1/0.2)
  --out <PATH>                Output file (default: stdout)

Exit codes: 0 success, 1 fatal error, 2 usage error, 3 some files or hands were rejected.";

enum Command {
    Import {
        input_dir: String,
        registry: ParserRegistry,
    },
    DeleteRun(Uuid),
    RebuildStats,
    Report {
        kind: ReportKind,
        stakes: Option<Vec<Stakes>>,
        out_path: Option<String>,
    },
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
    let mut rebuild_stats = false;
    let mut report_kind = None;
    let mut stakes = None;
    let mut out_path = None;
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
            "--rebuild-stats" => {
                if seen.iter().any(|f| f == "--rebuild-stats") {
                    return Err("--rebuild-stats given more than once".to_string());
                }
                seen.push("--rebuild-stats".to_string());
                rebuild_stats = true;
            }
            "--report" => {
                let raw = take_value(&mut args, "--report", &mut seen)?;
                let kind = raw.parse::<ReportKind>().map_err(|e| e.to_string())?;
                report_kind = Some(kind);
            }
            "--stakes" => {
                let raw = take_value(&mut args, "--stakes", &mut seen)?;
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    return Err("malformed --stakes: empty list".to_string());
                }
                let raw_items: Vec<&str> = trimmed.split(',').collect();
                if raw_items.is_empty() || raw_items.iter().any(|item| item.trim().is_empty()) {
                    return Err("malformed --stakes list".to_string());
                }
                let mut parsed_stakes = Vec::new();
                for item in raw_items {
                    let s = item.trim().parse::<Stakes>().map_err(|e| e.to_string())?;
                    parsed_stakes.push(s);
                }
                stakes = Some(parsed_stakes);
            }
            "--out" => {
                let path = take_value(&mut args, "--out", &mut seen)?;
                out_path = Some(path);
            }
            "-h" | "--help" => return Ok(Parsed::Help),
            flag if flag.starts_with("--") => return Err(format!("unknown option '{flag}'")),
            _ if input_dir.is_none() => input_dir = Some(arg),
            _ => return Err(format!("unexpected argument '{arg}'")),
        }
    }

    if let Some(kind) = report_kind {
        if rebuild_stats {
            return Err("--report cannot be combined with --rebuild-stats".to_string());
        }
        if delete_run.is_some() {
            return Err("--report cannot be combined with --delete-run".to_string());
        }
        if input_dir.is_some() {
            return Err("--report cannot be combined with an import".to_string());
        }
        if !registry.is_empty() {
            return Err("--report cannot be combined with site options".to_string());
        }
        return Ok(Parsed::Run(CliArgs {
            db_path,
            command: Command::Report {
                kind,
                stakes,
                out_path,
            },
        }));
    }

    if stakes.is_some() {
        return Err("--stakes requires --report".to_string());
    }
    if out_path.is_some() {
        return Err("--out requires --report".to_string());
    }

    let command = match (rebuild_stats, delete_run, input_dir) {
        (true, None, None) if registry.is_empty() => Command::RebuildStats,
        (true, _, _) => {
            return Err(
                "--rebuild-stats cannot be combined with an input directory or --delete-run"
                    .to_string(),
            );
        }
        (false, Some(run), None) if registry.is_empty() => Command::DeleteRun(run),
        (false, Some(_), _) => {
            return Err("--delete-run cannot be combined with an import".to_string());
        }
        (false, None, Some(input_dir)) => {
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
        (false, None, None) => return Err("missing <input-dir>".to_string()),
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
    if let Some(rebuild) = repo.rebuilt_on_open()
        && rebuild.hands > 0
    {
        eprintln!(
            "Statistics cache rebuilt for {} hand(s) (stats version {})",
            rebuild.hands, rebuild.version
        );
    }
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

fn rebuild_stats(db_path: &str) -> ExitCode {
    let mut repo = match SqliteRepository::open_existing(db_path) {
        Ok(r) => r,
        Err(err) => {
            eprintln!("Failed to open SQLite database at '{db_path}': {err}");
            return ExitCode::from(1);
        }
    };
    let outcome = match repo.rebuilt_on_open() {
        Some(rebuild) => rebuild,
        None => match repo.rebuild_derived() {
            Ok(rebuild) => rebuild,
            Err(err) => {
                eprintln!("Failed to rebuild statistics: {err}");
                return ExitCode::from(1);
            }
        },
    };
    println!(
        "Rebuilt statistics for {} hand(s) (stats version {})",
        outcome.hands, outcome.version
    );
    ExitCode::SUCCESS
}

fn import(db_path: &str, input_dir: &str, registry: &ParserRegistry) -> ExitCode {
    let mut repo = match SqliteRepository::open(db_path) {
        Ok(r) => r,
        Err(err) => {
            eprintln!("Failed to open SQLite database at '{db_path}': {err}");
            return ExitCode::from(1);
        }
    };
    if let Some(rebuild) = repo.rebuilt_on_open()
        && rebuild.hands > 0
    {
        eprintln!(
            "Statistics cache rebuilt for {} hand(s) (stats version {})",
            rebuild.hands, rebuild.version
        );
    }
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

fn run_report(
    db_path: &str,
    kind: ReportKind,
    stakes: Option<Vec<Stakes>>,
    out_path: Option<String>,
) -> ExitCode {
    let repo = match SqliteRepository::open_existing(db_path) {
        Ok(r) => r,
        Err(err) => {
            eprintln!("Failed to open SQLite database at '{db_path}': {err}");
            return ExitCode::from(1);
        }
    };
    if let Some(rebuild) = repo.rebuilt_on_open()
        && rebuild.hands > 0
    {
        eprintln!(
            "Statistics cache rebuilt for {} hand(s) (stats version {})",
            rebuild.hands, rebuild.version
        );
    }

    let options = ReportOptions { stakes };
    let mut buf = Vec::new();
    let outcome = match write_report(&repo, kind, &options, &mut buf) {
        Ok(o) => o,
        Err(err) => {
            eprintln!("Report failed: {err}");
            return ExitCode::from(1);
        }
    };

    for s in &outcome.unmatched_stakes {
        eprintln!("No hands at stakes {s}");
    }

    if let Some(path) = out_path {
        if let Err(err) = std::fs::write(&path, &buf) {
            eprintln!("Failed to write report to '{path}': {err}");
            return ExitCode::from(1);
        }
    } else if let Err(err) = std::io::stdout().write_all(&buf) {
        eprintln!("Failed to write report to stdout: {err}");
        return ExitCode::from(1);
    }

    ExitCode::SUCCESS
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
        Command::RebuildStats => rebuild_stats(&args.db_path),
        Command::Report {
            kind,
            stakes,
            out_path,
        } => run_report(&args.db_path, *kind, stakes.clone(), out_path.clone()),
    }
}
