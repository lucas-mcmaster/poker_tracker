//! Extracts and drift-checks the `## FILE: <path> · Milestone <N>` blocks of the
//! `poker_tracker` blueprint, so coding agents never retype code by hand.
//!
//! - `blueprint list <M>`: the files that make up milestone M and the variant of each.
//! - `blueprint extract <M>`: writes those files (the variant with the highest milestone <= M).
//! - `blueprint check <M>`: reports files that are missing, differ from the blueprint, or belong
//!   to a later milestone; exits with code 1 if there is any.
//!
//! Options: `--blueprint <file>` (default `docs/BLUEPRINT.md`), `--root <dir>` (default `.`).
//! Line endings are compared as LF, so a CRLF checkout is not reported as drift.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str =
    "usage: blueprint <list|extract|check> <milestone> [--blueprint <file>] [--root <dir>]";

/// One `## FILE:` block.
struct Block {
    path: String,
    milestone: u32,
    content: String,
}

fn parse_blueprint(text: &str) -> Result<Vec<Block>, String> {
    let lines: Vec<&str> = text
        .lines()
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    let mut blocks: Vec<Block> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some(header) = lines[i].strip_prefix("## FILE: ") else {
            i += 1;
            continue;
        };
        let (path, milestone) = header
            .split_once(" · Milestone ")
            .ok_or_else(|| format!("line {}: malformed FILE header", i + 1))?;
        let milestone: u32 = milestone
            .trim()
            .parse()
            .map_err(|_| format!("line {}: malformed milestone number", i + 1))?;
        let path = path.trim().to_string();
        let safe = Path::new(&path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)));
        if !safe {
            return Err(format!("line {}: unsafe path '{path}'", i + 1));
        }
        i += 1;
        while i < lines.len() && lines[i].trim().is_empty() {
            i += 1;
        }
        let fence = lines.get(i).copied().unwrap_or("");
        let ticks: String = fence.chars().take_while(|c| *c == '`').collect();
        if ticks.len() < 3 {
            return Err(format!("line {}: expected a code fence for {path}", i + 1));
        }
        i += 1;
        let start = i;
        while i < lines.len() && lines[i] != ticks {
            i += 1;
        }
        if i >= lines.len() {
            return Err(format!("unterminated code block for {path}"));
        }
        let mut content = lines[start..i].join("\n");
        content.push('\n');
        if blocks
            .iter()
            .any(|b| b.path == path && b.milestone == milestone)
        {
            return Err(format!(
                "duplicate block for {path} at milestone {milestone}"
            ));
        }
        blocks.push(Block {
            path,
            milestone,
            content,
        });
        i += 1;
    }
    if blocks.is_empty() {
        return Err("no ## FILE: blocks found".to_string());
    }
    Ok(blocks)
}

/// For milestone `m`: each path's variant with the highest milestone <= `m`.
fn select(blocks: &[Block], m: u32) -> BTreeMap<&str, &Block> {
    let mut chosen: BTreeMap<&str, &Block> = BTreeMap::new();
    for b in blocks.iter().filter(|b| b.milestone <= m) {
        let newer = chosen
            .get(b.path.as_str())
            .is_none_or(|current| current.milestone < b.milestone);
        if newer {
            chosen.insert(b.path.as_str(), b);
        }
    }
    chosen
}

fn run(args: &[String]) -> Result<bool, String> {
    let (command, milestone) = match args {
        [c, m, ..] => (c.as_str(), m),
        _ => return Err(USAGE.to_string()),
    };
    let milestone: u32 = milestone.parse().map_err(|_| USAGE.to_string())?;
    let mut blueprint = PathBuf::from("docs/BLUEPRINT.md");
    let mut root = PathBuf::from(".");
    let mut rest = args.iter().skip(2);
    while let Some(flag) = rest.next() {
        let value = rest.next().ok_or_else(|| USAGE.to_string())?;
        match flag.as_str() {
            "--blueprint" => blueprint = PathBuf::from(value),
            "--root" => root = PathBuf::from(value),
            _ => return Err(USAGE.to_string()),
        }
    }
    let text = fs::read_to_string(&blueprint)
        .map_err(|e| format!("cannot read {}: {e}", blueprint.display()))?;
    let blocks = parse_blueprint(&text)?;
    let chosen = select(&blocks, milestone);
    match command {
        "list" => {
            for (path, b) in &chosen {
                println!("M{}\t{path}", b.milestone);
            }
            Ok(true)
        }
        "extract" => {
            for (path, b) in &chosen {
                let dest = root.join(path);
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
                }
                fs::write(&dest, &b.content)
                    .map_err(|e| format!("cannot write {}: {e}", dest.display()))?;
                println!("wrote {path} (milestone {} variant)", b.milestone);
            }
            Ok(true)
        }
        "check" => {
            let mut clean = true;
            for (path, b) in &chosen {
                match fs::read_to_string(root.join(path)) {
                    Ok(found) if found.replace("\r\n", "\n") == b.content => {}
                    Ok(_) => {
                        clean = false;
                        println!(
                            "DIFFERS  {path} (expected the milestone {} variant)",
                            b.milestone
                        );
                    }
                    Err(_) => {
                        clean = false;
                        println!("MISSING  {path}");
                    }
                }
            }
            let mut later: Vec<&str> = blocks
                .iter()
                .filter(|b| !chosen.contains_key(b.path.as_str()))
                .map(|b| b.path.as_str())
                .collect();
            later.dedup();
            for path in later {
                if root.join(path).exists() {
                    clean = false;
                    println!("EARLY    {path} (belongs to a later milestone)");
                }
            }
            if clean {
                println!("OK: {} file(s) match milestone {milestone}", chosen.len());
            }
            Ok(clean)
        }
        _ => Err(USAGE.to_string()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const SAMPLE: &str = "intro\n\n## FILE: src/lib.rs · Milestone 1\n\n```rust\npub mod a;\n```\n\n---\n\n## FILE: src/lib.rs · Milestone 3\n\n```rust\npub mod a;\npub mod b;\n```\n\n## FILE: src/b.rs · Milestone 3\n\n```rust\n// b\n```\n";

    #[test]
    fn selects_the_latest_variant_not_after_the_milestone() {
        let blocks = parse_blueprint(SAMPLE).unwrap();
        let m1 = select(&blocks, 1);
        assert_eq!(m1.len(), 1);
        assert_eq!(m1["src/lib.rs"].content, "pub mod a;\n");
        let m3 = select(&blocks, 5);
        assert_eq!(m3["src/lib.rs"].content, "pub mod a;\npub mod b;\n");
        assert_eq!(m3["src/b.rs"].milestone, 3);
    }

    #[test]
    fn rejects_unsafe_paths_and_crlf_is_tolerated() {
        assert!(parse_blueprint("## FILE: ../x.rs · Milestone 1\n\n```\nx\n```\n").is_err());
        let blocks = parse_blueprint(&SAMPLE.replace('\n', "\r\n")).unwrap();
        assert_eq!(select(&blocks, 1)["src/lib.rs"].content, "pub mod a;\n");
    }
}
