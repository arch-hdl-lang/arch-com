#[path = "../build_version.rs"]
mod build_version;
use std::{fs, path::Path, process::Command};
fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=Version Test",
            "-c",
            "user.email=version@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}
#[test]
fn provenance_tracks_clean_dirty_staged_restored_and_worktree_heads() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    fs::create_dir(&root).unwrap();
    git(&root, &["init", "-b", "main"]);
    fs::write(root.join("source"), "original").unwrap();
    git(&root, &["add", "source"]);
    git(&root, &["commit", "-m", "Initial fixture"]);
    let head = git(&root, &["rev-parse", "--short=12", "HEAD"]);
    assert_eq!(
        build_version::version(&root, "0.72.5"),
        format!("0.72.5 (git {head})")
    );
    fs::write(root.join("source"), "edit").unwrap();
    assert!(build_version::version(&root, "0.72.5").ends_with("+dirty)"));
    git(&root, &["add", "source"]);
    assert!(build_version::version(&root, "0.72.5").ends_with("+dirty)"));
    git(&root, &["reset", "--hard", "HEAD"]);
    assert!(!build_version::version(&root, "0.72.5").contains("dirty"));
    let wt = tmp.path().join("worktree");
    git(
        &root,
        &["worktree", "add", "--detach", wt.to_str().unwrap(), "HEAD"],
    );
    assert_eq!(
        build_version::version(&wt, "0.72.5"),
        format!("0.72.5 (git {head})")
    );
    let watches = build_version::watch_paths(&wt);
    assert!(watches.contains(&wt.join("source")));
    assert!(watches
        .iter()
        .any(|p| p.ends_with("HEAD") && p.to_string_lossy().contains("worktrees")));
    fs::write(wt.join("source"), "new revision").unwrap();
    git(&wt, &["commit", "-am", "Second fixture"]);
    assert_ne!(
        build_version::version(&wt, "0.72.5"),
        build_version::version(&root, "0.72.5")
    );
    let archive = root.join("source-archive");
    fs::create_dir(&archive).unwrap();
    assert_eq!(
        build_version::version(&archive, "0.72.5"),
        "0.72.5 (git unknown)"
    );
}
