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

#[test]
fn report_cli_paths_options_and_errors() {
    let cwd = TempDir::new().unwrap();

    // 1. --report on a missing database exits 1 and creates no file
    let (code, _, err) = run(
        cwd.path(),
        &["--report", "results", "--db", "missing.sqlite"],
    );
    assert_eq!(code, 1);
    assert!(
        err.contains("no database exists") || err.contains("Failed to open SQLite database"),
        "stderr: {err}"
    );
    assert!(!cwd.path().join("missing.sqlite").exists());

    // 2. Unknown report kind exits 2
    let (code, _, err) = run(cwd.path(), &["--report", "unknown_kind"]);
    assert_eq!(code, 2, "must exit 2 on unknown report kind: {err}");
    assert!(err.contains("unknown report kind"), "stderr: {err}");

    // 3. Combined usage errors (exit 2)
    let exports = cwd.path().join("exports");
    fs::create_dir(&exports).unwrap();
    fs::write(exports.join("s.txt"), SAMPLE_SESSION).unwrap();

    let (code, _, err) = run(
        cwd.path(),
        &[
            "exports",
            "--ggpoker-currency",
            "CAD",
            "--report",
            "results",
        ],
    );
    assert_eq!(code, 2, "import + report must exit 2: {err}");

    let (code, _, err) = run(
        cwd.path(),
        &[
            "--delete-run",
            "0194bf00-0000-7000-8000-000000000000",
            "--report",
            "results",
        ],
    );
    assert_eq!(code, 2, "delete-run + report must exit 2: {err}");

    let (code, _, err) = run(cwd.path(), &["--rebuild-stats", "--report", "results"]);
    assert_eq!(code, 2, "rebuild-stats + report must exit 2: {err}");

    // 4. --stakes or --out without --report exits 2
    let (code, _, err) = run(cwd.path(), &["--stakes", "0.05/0.1"]);
    assert_eq!(code, 2, "--stakes without --report must exit 2: {err}");

    let (code, _, err) = run(cwd.path(), &["--out", "report.csv"]);
    assert_eq!(code, 2, "--out without --report must exit 2: {err}");

    // 5. Malformed stakes exit 2
    for bad in ["", "0.05", "0.05/", "a/b", "0/0.1", "0.0000001/0.1"] {
        let (code, _, _) = run(cwd.path(), &["--report", "results", "--stakes", bad]);
        assert_eq!(code, 2, "malformed stakes '{bad}' must exit 2");
    }

    // 6. Import session to test valid reports
    let (code, _, _) = run(cwd.path(), &["exports", "--ggpoker-currency", "CAD"]);
    assert_eq!(code, 0);

    // 7. stdout carries CSV only
    let (code, out, err) = run(cwd.path(), &["--report", "results"]);
    assert_eq!(code, 0, "stderr: {err}");
    assert!(out.starts_with("currency,stakes,hands,net,"));
    assert!(err.is_empty(), "expected empty stderr, got: {err}");

    // 8. --out writes to file, stdout is empty
    let out_file = cwd.path().join("results.csv");
    let (code, out, err) = run(
        cwd.path(),
        &["--report", "results", "--out", out_file.to_str().unwrap()],
    );
    assert_eq!(code, 0, "stderr: {err}");
    assert!(out.is_empty(), "stdout must be empty with --out");
    assert!(out_file.exists());
    let file_content = fs::read_to_string(&out_file).unwrap();
    assert!(file_content.starts_with("currency,stakes,hands,net,"));

    // 9. Unmatched stakes prints to stderr, exits 0
    let (code, out, err) = run(cwd.path(), &["--report", "results", "--stakes", "1/2"]);
    assert_eq!(code, 0);
    assert!(err.contains("No hands at stakes 1/2"), "stderr was: {err}");
    assert_eq!(out.lines().count(), 1);

    // 10. Empty database prints header only
    let empty_db = cwd.path().join("empty.sqlite");
    {
        let _ = poker_tracker::storage::SqliteRepository::open(&empty_db).unwrap();
    }
    let (code, out, err) = run(
        cwd.path(),
        &["--report", "results", "--db", empty_db.to_str().unwrap()],
    );
    assert_eq!(code, 0, "stderr: {err}");
    assert_eq!(out.lines().count(), 1);
}

#[test]
fn import_into_a_foreign_database_exits_1_and_a_rebuilt_cache_is_announced_on_stderr() {
    let cwd = TempDir::new().unwrap();
    let exports = cwd.path().join("exports");
    fs::create_dir(&exports).unwrap();
    fs::write(exports.join("s.txt"), SAMPLE_SESSION).unwrap();

    // A database that is not a poker_tracker database cannot be opened: fatal, exit 1 (AGENTS §6)
    let foreign = cwd.path().join("foreign.sqlite");
    rusqlite::Connection::open(&foreign)
        .unwrap()
        .execute_batch("CREATE TABLE notes (x TEXT);")
        .unwrap();
    let (code, out, err) = run(
        cwd.path(),
        &[
            "exports",
            "--ggpoker-currency",
            "CAD",
            "--db",
            "foreign.sqlite",
        ],
    );
    assert_eq!(code, 1, "{out}{err}");
    assert!(err.contains("Failed to open SQLite database"), "{err}");

    // SPEC_v3.7 M13.5: a command whose open rebuilt the cache with at least one hand says so on
    // stderr first; stdout of --report holds only the CSV.
    let (code, _, err) = run(cwd.path(), &["exports", "--ggpoker-currency", "CAD"]);
    assert_eq!(code, 0, "{err}");
    let forget_cache = || {
        rusqlite::Connection::open(cwd.path().join("poker_tracker.sqlite"))
            .unwrap()
            .execute("DELETE FROM derived_cache", [])
            .unwrap();
    };
    forget_cache();
    let (code, out, err) = run(cwd.path(), &["--report", "results"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(
        err,
        "Statistics cache rebuilt for 2 hand(s) (stats version 4)\n"
    );
    assert!(out.starts_with("currency,stakes,hands,net,"), "{out}");
    forget_cache();
    let (code, _, err) = run(cwd.path(), &["exports", "--ggpoker-currency", "CAD"]);
    assert_eq!(code, 0, "{err}");
    assert!(
        err.starts_with("Statistics cache rebuilt for 2 hand(s) (stats version 4)\n"),
        "{err}"
    );
}

#[test]
fn option_combinations_order_and_rebuild_notices() {
    let cwd = TempDir::new().unwrap();
    let exports = cwd.path().join("exports");
    fs::create_dir(&exports).unwrap();
    fs::write(exports.join("s.txt"), SAMPLE_SESSION).unwrap();

    // --rebuild-stats and --delete-run take no site option (SPEC_v3.7 M13.5; AGENTS §6)
    for args in [
        &["--rebuild-stats", "--ggpoker-currency", "CAD"][..],
        &[
            "--delete-run",
            "0194bf00-0000-7000-8000-000000000000",
            "--ggpoker-currency",
            "CAD",
        ],
        &["--rebuild-stats", "--rebuild-stats"],
    ] {
        let (code, _, err) = run(cwd.path(), args);
        assert_eq!(code, 2, "{args:?}: {err}");
    }
    // Option order does not matter: --db before --rebuild-stats is a missing database (exit 1)
    let (code, _, err) = run(cwd.path(), &["--db", "missing.sqlite", "--rebuild-stats"]);
    assert_eq!(code, 1, "{err}");
    // Usage errors say what is wrong
    let (_, _, err) = run(cwd.path(), &["--bogus"]);
    assert!(err.contains("unknown option '--bogus'"), "{err}");
    let (_, _, err) = run(
        cwd.path(),
        &["exports", "other", "--ggpoker-currency", "CAD"],
    );
    assert!(err.contains("unexpected argument 'other'"), "{err}");

    // A new, empty database prints no rebuild notice (M13.5); nor does a rebuild of no hands.
    let (code, _, err) = run(cwd.path(), &["exports", "--ggpoker-currency", "CAD"]);
    assert_eq!(code, 0, "{err}");
    assert!(!err.contains("Statistics cache rebuilt"), "{err}");
    let empty = cwd.path().join("empty.sqlite");
    drop(poker_tracker::storage::SqliteRepository::open(&empty).unwrap());
    let forget_cache = |db: &Path| {
        rusqlite::Connection::open(db)
            .unwrap()
            .execute("DELETE FROM derived_cache", [])
            .unwrap();
    };
    forget_cache(&empty);
    let (code, _, err) = run(
        cwd.path(),
        &["--report", "results", "--db", empty.to_str().unwrap()],
    );
    assert_eq!(code, 0, "{err}");
    assert!(err.is_empty(), "{err}");

    // --delete-run announces a rebuild of stored hands like the other commands
    forget_cache(&cwd.path().join("poker_tracker.sqlite"));
    let (code, _, err) = run(
        cwd.path(),
        &["--delete-run", "0194bf00-0000-7000-8000-000000000000"],
    );
    assert_eq!(code, 1, "unknown run");
    assert!(
        err.starts_with("Statistics cache rebuilt for 2 hand(s) (stats version 4)\n"),
        "{err}"
    );
}

#[cfg(unix)]
#[test]
fn non_unicode_cli_arguments() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let dir = tempfile::TempDir::new().unwrap();
    let exports = dir.path().join("exports");
    fs::create_dir(&exports).unwrap();
    fs::write(exports.join("s.txt"), SAMPLE_SESSION).unwrap();

    let db_path = dir.path().join(OsStr::from_bytes(b"caf\xE9.sqlite"));

    let run_os = |args: &[&OsStr]| -> (i32, String, String) {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_poker_tracker"))
            .current_dir(dir.path())
            .args(args)
            .output()
            .unwrap();
        (
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    };

    // 1. --rebuild-stats on missing db with non-Unicode path exits 1 (fatal: not found), not 101 panic.
    let (code, _, _) = run_os(&[
        OsStr::new("--rebuild-stats"),
        OsStr::new("--db"),
        db_path.as_os_str(),
    ]);
    assert_eq!(code, 1);

    // 2. import into non-Unicode db path exits 0 and creates the file.
    let (code, _, _) = run_os(&[
        exports.as_os_str(),
        OsStr::new("--ggpoker-currency"),
        OsStr::new("CAD"),
        OsStr::new("--db"),
        db_path.as_os_str(),
    ]);
    assert_eq!(code, 0);
    assert!(db_path.exists());

    // 3. non-Unicode value for text option --report exits 2 (usage error).
    let invalid_report = OsStr::from_bytes(b"r\xE9sults");
    let (code, _, _) = run_os(&[
        OsStr::new("--report"),
        invalid_report,
        OsStr::new("--db"),
        db_path.as_os_str(),
    ]);
    assert_eq!(code, 2);
}
