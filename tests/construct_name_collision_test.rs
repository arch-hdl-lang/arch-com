//! Guard against ambiguous `inst` resolution (#814).
//!
//! `inst c: X` is resolved by looking for a file literally called `X.arch` in
//! the same directory. When a *second* file there also defines `X`, the
//! compiler takes the filename match and never mentions the other one, so the
//! instance silently binds to a module the author did not mean. Since #797
//! that surfaces as a confidently-wrong "`p` is not a port of `X`" naming
//! ports that do exist — on the other `X`.
//!
//! Two files defining the same construct name is not itself a problem: most
//! of the corpus is standalone single-file cases, and 156 `verilog_eval`
//! problems all declare `TopModule` by design. It only becomes one when some
//! file in the same directory *instantiates* the ambiguous name. The fix in
//! that case is an explicit `use <file>;`, which takes precedence over
//! filename discovery.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[test]
fn ambiguous_inst_targets_are_disambiguated_by_use() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut dirs: Vec<PathBuf> = Vec::new();
    for top in ["tests", "examples"] {
        collect_dirs(&root.join(top), &mut dirs);
    }
    dirs.sort();

    let mut offenders: Vec<String> = Vec::new();
    let mut checked_dirs = 0usize;

    for dir in &dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "arch"))
            .collect();
        if files.len() < 2 {
            continue;
        }
        files.sort();
        checked_dirs += 1;

        let parsed: Vec<(String, Source)> = files
            .iter()
            .filter_map(|f| {
                let text = std::fs::read_to_string(f).ok()?;
                let name = f.file_name()?.to_string_lossy().to_string();
                Some((name, Source::scan(&text)))
            })
            .collect();

        // construct name -> files defining it, within this directory
        let mut defs: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (file, src) in &parsed {
            for c in &src.constructs {
                defs.entry(c.as_str()).or_default().push(file.as_str());
            }
        }

        let rel = dir
            .strip_prefix(&root)
            .unwrap_or(dir)
            .to_string_lossy()
            .replace('\\', "/");

        for (file, src) in &parsed {
            for target in &src.insts {
                // Defined locally: no discovery happens, no ambiguity.
                if src.constructs.contains(target) {
                    continue;
                }
                let Some(providers) = defs.get(target.as_str()) else {
                    continue;
                };
                if providers.len() < 2 {
                    continue;
                }
                // Discovery only fires when a file is literally named after
                // the construct. Where it can't (e.g. tests/aes declares
                // PascalCase constructs in snake_case files), the unit is
                // built by passing sources together and there is nothing to
                // resolve ambiguously.
                let discoverable = format!("{target}.arch");
                if !providers.iter().any(|p| *p == discoverable) {
                    continue;
                }
                // An explicit `use` names the provider and wins over the
                // filename lookup, so the binding is unambiguous.
                if !src.uses.is_empty() {
                    continue;
                }
                offenders.push(format!(
                    "{rel}/{file}: `inst … : {target}` is ambiguous — {} both define it; \
                     add `use <file>;` to name the intended one",
                    providers.join(" and ")
                ));
            }
        }
    }

    assert!(
        checked_dirs > 5,
        "expected several multi-file directories, saw {checked_dirs} — did the layout change?"
    );
    assert!(
        offenders.is_empty(),
        "{} ambiguous inst target(s); resolution would silently pick the \
         filename match.\n{}",
        offenders.len(),
        offenders.join("\n")
    );
}

struct Source {
    constructs: BTreeSet<String>,
    insts: BTreeSet<String>,
    uses: BTreeSet<String>,
}

impl Source {
    /// Column-0 construct declarations, `inst <name>: <Target>` targets, and
    /// `use <file>;` imports. A lint-grade scan: every construct in the corpus
    /// is declared at column 0, and `inst` lines are unambiguous enough that a
    /// full parse would not change the result.
    fn scan(text: &str) -> Self {
        const KEYWORDS: &[&str] = &[
            "module",
            "fsm",
            "pipeline",
            "fifo",
            "ram",
            "cam",
            "counter",
            "arbiter",
            "regfile",
            "linklist",
            "bus",
            "synchronizer",
            "clkgate",
            "template",
        ];
        let ident = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_');

        let mut constructs = BTreeSet::new();
        let mut insts = BTreeSet::new();
        let mut uses = BTreeSet::new();

        for line in text.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("use ") {
                let name = rest.trim_end_matches(';').trim();
                if ident(name) {
                    uses.insert(name.to_string());
                }
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("inst ") {
                if let Some((_, target)) = rest.split_once(':') {
                    let target = target.trim();
                    if ident(target) {
                        insts.insert(target.to_string());
                    }
                }
                continue;
            }
            // Declarations sit at column 0; nested blocks are indented.
            if line.starts_with(char::is_whitespace) {
                continue;
            }
            let mut parts = line.split_whitespace();
            if let (Some(kw), Some(name)) = (parts.next(), parts.next()) {
                if KEYWORDS.contains(&kw) && ident(name) {
                    constructs.insert(name.to_string());
                }
            }
        }

        Source {
            constructs,
            insts,
            uses,
        }
    }
}

fn collect_dirs(dir: &Path, out: &mut Vec<PathBuf>) {
    if !dir.is_dir() {
        return;
    }
    out.push(dir.to_path_buf());
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect_dirs(&path, out);
        }
    }
}
