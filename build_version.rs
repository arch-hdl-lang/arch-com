//! Build-time provenance. A source archive without Git reports `unknown`.
use std::path::{Path, PathBuf};
use std::process::Command;

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

pub fn version(root: &Path, semver: &str) -> String {
    // Do not attribute an unpacked source archive to an unrelated parent repo.
    let is_root = git(root, &["rev-parse", "--show-toplevel"])
        .and_then(|p| PathBuf::from(p).canonicalize().ok())
        == root.canonicalize().ok();
    if !is_root {
        return format!("{semver} (git unknown)");
    }
    let Some(commit) = git(root, &["rev-parse", "--short=12", "HEAD"]) else {
        return format!("{semver} (git unknown)");
    };
    // Like git describe --dirty, this covers staged/unstaged tracked changes.
    let dirty = git(root, &["status", "--porcelain", "--untracked-files=no"])
        .map(|s| !s.is_empty())
        .unwrap_or(true);
    format!(
        "{semver} (git {commit}{})",
        if dirty { "+dirty" } else { "" }
    )
}

pub fn watch_paths(root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    // Resolve Git administrative paths rather than assuming .git is a directory:
    // linked worktrees have their own HEAD/index and a shared refs directory.
    for name in ["HEAD", "index", "packed-refs"] {
        if let Some(path) = git(
            root,
            &["rev-parse", "--path-format=absolute", "--git-path", name],
        ) {
            paths.push(PathBuf::from(path));
        }
    }
    if let Some(reference) = git(root, &["symbolic-ref", "-q", "HEAD"]) {
        if let Some(path) = git(
            root,
            &[
                "rev-parse",
                "--path-format=absolute",
                "--git-path",
                &reference,
            ],
        ) {
            paths.push(PathBuf::from(path));
        }
    }
    // Re-evaluate dirty state on tracked edits/restores, even without a commit.
    if let Some(files) = git(root, &["ls-files"]) {
        paths.extend(files.lines().map(|p| root.join(p)));
    }
    paths
}
