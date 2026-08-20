//! Every `.archi` interface stub `arch build` emits must be a valid input to
//! the compiler that emitted it (#815).
//!
//! The failure mode this guards is quiet: a stub that drops part of a
//! construct's surface still *parses*, so nothing complains until a consumer
//! tries to connect the missing pins — and since #797 landed, that surfaces as
//! a confidently-wrong "not a port of" error against a port that does exist.
//!
//! The sweep is corpus-driven rather than a fixed fixture list so a newly added
//! construct kind is covered automatically.

use std::path::{Path, PathBuf};
use std::process::Command;

fn arch() -> &'static str {
    env!("CARGO_BIN_EXE_arch")
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// `arch build` the source into `work`, then `arch check` each emitted
/// `.archi` on its own. Returns one message per stub that fails to re-check.
fn roundtrip_failures(source: &Path, work: &Path) -> Vec<String> {
    std::fs::create_dir_all(work).expect("mkdir work");
    let file_name = source.file_name().expect("file name");
    std::fs::copy(source, work.join(file_name)).expect("copy source");

    let built = Command::new(arch())
        .current_dir(work)
        .args(["build", &file_name.to_string_lossy(), "-o", "out.sv"])
        .output()
        .expect("run arch build");
    if !built.status.success() {
        // A source that does not build is out of scope here; other suites
        // cover that. Only the emitted stubs are under test.
        return Vec::new();
    }

    let mut failures = Vec::new();
    let mut stubs: Vec<PathBuf> = std::fs::read_dir(work)
        .expect("read work")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "archi"))
        .collect();
    stubs.sort();

    for stub in stubs {
        let stub_name = stub
            .file_name()
            .expect("stub name")
            .to_string_lossy()
            .to_string();
        let out = Command::new(arch())
            .current_dir(work)
            .args(["check", &stub_name])
            .output()
            .expect("run arch check");
        if !out.status.success() {
            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            let detail = combined
                .lines()
                .find(|l| l.contains('×'))
                .unwrap_or("<no diagnostic>")
                .trim()
                .to_string();
            failures.push(format!("{stub_name} (from {}): {detail}", source.display()));
        }
    }
    failures
}

#[test]
fn every_emitted_archi_stub_reparses() {
    let root = repo_root();
    let tmp = std::env::temp_dir().join(format!("arch_archi_roundtrip_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);

    let mut sources: Vec<PathBuf> = Vec::new();
    for dir in ["tests", "examples"] {
        collect_arch_sources(&root.join(dir), &mut sources);
    }
    sources.sort();
    assert!(
        sources.len() > 50,
        "expected a substantial corpus, found {} sources — did the layout change?",
        sources.len()
    );

    let mut failures = Vec::new();
    for (idx, source) in sources.iter().enumerate() {
        failures.extend(roundtrip_failures(source, &tmp.join(idx.to_string())));
    }
    let _ = std::fs::remove_dir_all(&tmp);

    assert!(
        failures.is_empty(),
        "{} emitted .archi stub(s) do not re-check.\n\
         An interface stub must be a valid input to the compiler that wrote it.\n{}",
        failures.len(),
        failures.join("\n")
    );
}

fn collect_arch_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            // One level of nesting covers examples/nic400 etc. without
            // dragging in the large generated corpora under tests/.
            if dir.ends_with("examples") {
                collect_arch_sources(&path, out);
            }
        } else if path.extension().is_some_and(|x| x == "arch") {
            out.push(path);
        }
    }
}

/// A `package` has no `.archi` form, and emitting one was worse than emitting
/// nothing: the file could not be consumed by any code path (#819).
///
/// `use` resolves only `<name>.arch`, and the `.archi`-aware `inst` / bus
/// lookups key on the *construct* filename rather than the package's, so a
/// `TestPkg.archi` was written and never read. A package also cannot declare a
/// module — the parser accepts only param / domain / enum / struct / bus /
/// function — so it never takes part in the module-name resolution `.archi`
/// exists for.
#[test]
fn packages_emit_no_archi_but_still_emit_sv_and_resolve() {
    let root = repo_root();
    let tmp = std::env::temp_dir().join(format!("arch_pkg_no_archi_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("mkdir");
    for f in ["TestPkg.arch", "pkg_user.arch"] {
        std::fs::copy(root.join("examples").join(f), tmp.join(f)).expect("copy fixture");
    }

    let out = Command::new(arch())
        .current_dir(&tmp)
        .args(["build", "TestPkg.arch", "-o", "TestPkg.sv"])
        .output()
        .expect("run arch build");
    assert!(out.status.success(), "package build failed");

    let stubs: Vec<String> = std::fs::read_dir(&tmp)
        .expect("read dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "archi"))
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert!(
        stubs.is_empty(),
        "a package must not emit an .archi, got {stubs:?}"
    );

    // The SV package is still emitted — this removes the interface stub only.
    let sv = std::fs::read_to_string(tmp.join("TestPkg.sv")).expect("read sv");
    assert!(
        sv.contains("package TestPkg"),
        "SV package body should still be emitted:\n{sv}"
    );

    // And a consumer still resolves the package through its `.arch`.
    let out = Command::new(arch())
        .current_dir(&tmp)
        .args(["check", "pkg_user.arch"])
        .output()
        .expect("run arch check");
    assert!(
        out.status.success(),
        "package consumer should still check clean: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let _ = std::fs::remove_dir_all(&tmp);
}
