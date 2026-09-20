//! CLI regression: filter event kinds before top-K and count only returned hits.
use std::{fs, process::Command};

fn run(home: &std::path::Path, args: &[&str]) -> std::process::Output {
    let output = Command::new(env!("CARGO_BIN_EXE_arch"))
        .env("HOME", home)
        .env("ARCH_NO_LEARN", "1")
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    output
}

fn check_filter(feature: bool, indexed: bool) {
    let home = tempfile::tempdir().unwrap();
    let store = home.path().join(".arch/learn");
    fs::create_dir_all(&store).unwrap();
    let excluded = if feature { "error_fix" } else { "feature" };
    let eligible = if feature { "feature" } else { "error_fix" };
    let mut events = Vec::new();
    // More than the previous --feature top*8 pool, all scoring above the hits.
    for i in 0..30 {
        events.push(serde_json::json!({
            "ts": format!("excluded-{i}"), "kind": excluded,
            "error_code": "clock", "error_message": "clock",
            "diff_summary": "clock", "file_path": "excluded.arch",
            "src_before": "", "src_after": ""
        }));
    }
    for i in 0..3 {
        events.push(serde_json::json!({
            "ts": format!("eligible-{i}"), "kind": eligible,
            "error_code": "parse", "error_message": "clock divider reset async",
            "diff_summary": "add missing declaration", "file_path": "eligible.arch",
            "src_before": "", "src_after": ""
        }));
    }
    fs::write(
        store.join("events.jsonl"),
        events.iter().map(|e| format!("{e}\n")).collect::<String>(),
    )
    .unwrap();
    if indexed {
        run(home.path(), &["learn-index"]);
    }
    let mut args = vec!["advise", "-k", "2", "clock"];
    if feature {
        args.push("--feature");
    }
    let output = run(home.path(), &args);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.matches("── match #").count(), 2, "{stdout}");
    assert!(!stdout.contains("excluded.arch"), "{stdout}");
    assert!(stdout.contains("eligible.arch"), "{stdout}");
    let counts: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(store.join("retrieval_counts.json")).unwrap())
            .unwrap();
    assert_eq!(counts.as_object().unwrap().len(), 2);
    assert!(counts.as_object().unwrap().values().all(|v| v == 1));
    // Zero results and unmatched queries must not inflate retrieval counts.
    run(home.path(), &["advise", "-k", "0", "clock"]);
    run(home.path(), &["advise", "unrelatedquery"]);
    let after: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(store.join("retrieval_counts.json")).unwrap())
            .unwrap();
    assert_eq!(counts, after);
}

#[test]
fn default_advice_does_not_starve_behind_features() {
    for indexed in [false, true] {
        check_filter(false, indexed);
    }
}

#[test]
fn feature_advice_does_not_starve_behind_errors() {
    for indexed in [false, true] {
        check_filter(true, indexed);
    }
}
