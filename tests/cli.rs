//! End-to-end tests of the `poker_tracker` binary: exit codes, messages and side effects.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

use common::SAMPLE_SESSION;

fn run(cwd: &Path, args: &[&str]) -> (i32, String, String) {
    let Output {
        status,
        stdout,
        stderr,
    } = Command::new(env!("CARGO_BIN_EXE_poker_tracker"))
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap();
    (
        status.code().unwrap(),
        String::from_utf8(stdout).unwrap(),
        String::from_utf8(stderr).unwrap(),
    )
}

fn run_id(stdout: &str) -> String {
    stdout
        .lines()
        .find_map(|l| l.strip_prefix("Batch Run ID       : "))
        .unwrap()
        .to_string()
}

#[test]
fn help_and_usage_errors_exit_with_their_codes_and_create_nothing() {
    let cwd = TempDir::new().unwrap();
    let (code, out, _) = run(cwd.path(), &["--help"]);
    assert_eq!(code, 0);
    assert!(out.contains("Usage:"));
    for args in [
        &["missing_dir", "--ggpoker-currency", "CAD"][..],
        &[
            ".",
            "--ggpoker-currency",
            "CAD",
            "--ggpoker-currency",
            "USD",
        ],
        &[".", "--bogus"],
        &["."],
        &[".", "--ggpoker-currency", "C$"],
    ] {
        let (code, _, err) = run(cwd.path(), args);
        assert_eq!(code, 2, "{args:?}: {err}");
    }
    let (_, _, err) = run(cwd.path(), &["missing_dir", "--ggpoker-currency", "CAD"]);
    assert!(err.contains("does not exist"), "{err}");
    assert!(
        fs::read_dir(cwd.path()).unwrap().next().is_none(),
        "no database may be created by a usage error"
    );
}

#[test]
fn imports_reimports_and_undoes_runs_from_the_command_line() {
    let cwd = TempDir::new().unwrap();
    let exports = cwd.path().join("exports");
    fs::create_dir(&exports).unwrap();
    fs::write(exports.join("s.txt"), SAMPLE_SESSION).unwrap();

    // The currency code is accepted in any letter case.
    let (code, out, err) = run(cwd.path(), &["exports", "--ggpoker-currency", "cad"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("Hands Ingested     : 2"), "{out}");
    let first = run_id(&out);
    let (code, out, _) = run(cwd.path(), &["exports", "--ggpoker-currency", "CAD"]);
    assert_eq!(code, 0);
    assert!(out.contains("Duplicates Skipped : 2"), "{out}");
    let second = run_id(&out);

    // Undoing the first run keeps the hands the second run also contained.
    let (code, out, _) = run(cwd.path(), &["--delete-run", &first]);
    assert_eq!(code, 0);
    assert!(
        out.contains("Deleted 0 hand(s)") && out.contains("kept 2 hand(s)"),
        "{out}"
    );
    let (code, _, err) = run(cwd.path(), &["--delete-run", &first]);
    assert_eq!(code, 1);
    assert!(err.contains("does not exist"), "{err}");
    let (code, out, _) = run(cwd.path(), &["--delete-run", &second]);
    assert_eq!(code, 0);
    assert!(out.contains("Deleted 2 hand(s)"), "{out}");

    // Deleting from a database that does not exist neither succeeds nor creates it.
    let (code, _, _) = run(
        cwd.path(),
        &["--delete-run", &second, "--db", "other.sqlite"],
    );
    assert_eq!(code, 1);
    assert!(!cwd.path().join("other.sqlite").exists());

    // A partial import and an empty input both exit 3.
    let broken = SAMPLE_SESSION.replace("Dealt to Hero [5h 2s]", "Dealt to Hero [1h 2s]");
    fs::write(exports.join("s.txt"), broken).unwrap();
    let (code, out, _) = run(cwd.path(), &["exports", "--ggpoker-currency", "CAD"]);
    assert_eq!(code, 3);
    assert!(out.contains("Hands Quarantined  : 1"), "{out}");
    let empty = cwd.path().join("empty");
    fs::create_dir(&empty).unwrap();
    let (code, _, err) = run(cwd.path(), &["empty", "--ggpoker-currency", "CAD"]);
    assert_eq!(code, 3);
    assert!(err.contains("no hand-history files"), "{err}");
}

#[test]
fn rebuild_stats_cli_paths_and_errors() {
    let cwd = TempDir::new().unwrap();

    // --rebuild-stats on a missing file exits 1
    let (code, _, err) = run(cwd.path(), &["--rebuild-stats", "--db", "missing.sqlite"]);
    assert_eq!(code, 1);
    assert!(
        err.contains("Failed to open SQLite database") || err.contains("no database exists"),
        "{err}"
    );
    assert!(!cwd.path().join("missing.sqlite").exists());

    // --rebuild-stats combined with an input directory exits 2
    let exports = cwd.path().join("exports");
    fs::create_dir(&exports).unwrap();
    fs::write(exports.join("s.txt"), SAMPLE_SESSION).unwrap();
    let (code, _, err) = run(
        cwd.path(),
        &["exports", "--rebuild-stats", "--ggpoker-currency", "CAD"],
    );
    assert_eq!(code, 2, "must exit 2 on combined usage: {err}");

    // Also combined with --delete-run exits 2
    let (code, _, err) = run(
        cwd.path(),
        &[
            "--rebuild-stats",
            "--delete-run",
            "0194bf00-0000-7000-8000-000000000000",
        ],
    );
    assert_eq!(code, 2, "must exit 2 on combined usage: {err}");

    // Successful rebuild prints to stdout and exits 0
    let (code, _, _) = run(cwd.path(), &["exports", "--ggpoker-currency", "CAD"]);
    assert_eq!(code, 0);

    let (code, out, err) = run(cwd.path(), &["--rebuild-stats"]);
    assert_eq!(code, 0, "stderr: {err}");
    let expected_msg = format!(
        "Rebuilt statistics for 2 hand(s) (stats version {})",
        poker_tracker::stats::STATS_VERSION
    );
    assert!(out.contains(&expected_msg), "stdout was: {out}");
}
