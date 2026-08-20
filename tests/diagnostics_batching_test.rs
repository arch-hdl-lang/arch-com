//! `arch check` reports every error a pass found, not just the first (#750).
//!
//! The type checker has always accumulated into a `Vec<CompileError>`; the
//! reporting boundary in `main.rs` kept element 0 and dropped the rest, so a
//! file with N independent errors cost N compile-and-fix round trips. These
//! tests drive the real binary, because the truncation lived in the CLI
//! reporting path — an in-process check of `TypeChecker::check()` would have
//! passed the whole time.

use std::path::PathBuf;
use std::process::Command;

fn arch() -> &'static str {
    env!("CARGO_BIN_EXE_arch")
}

fn workdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("arch_diag_batch_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// stderr with environment-dependent chatter removed — the learn-store warning
/// and the `arch advise` hint depend on the developer's `~/.arch` and would
/// make output comparisons flaky.
fn check_output(dir: &std::path::Path, files: &[&str]) -> String {
    let out = Command::new(arch())
        .current_dir(dir)
        .arg("check")
        .args(files)
        .output()
        .expect("run arch check");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    combined
        .lines()
        .filter(|l| !l.contains("learn store") && !l.contains("arch advise"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Two independent width errors, the reproducer from #750.
const TWO_ERRORS: &str = "\
module Two
  port a: in UInt<8>;
  port b: in UInt<8>;
  port o1: out UInt<8>;
  port o2: out UInt<8>;
  comb
    o1 = a + b;
    o2 = a + b;
  end comb
end module Two
";

#[test]
fn independent_errors_are_all_reported_in_source_order() {
    let dir = workdir("order");
    std::fs::write(dir.join("Two.arch"), TWO_ERRORS).expect("write");
    let out = check_output(&dir, &["Two.arch"]);

    let o1 = out.find("`o1`").unwrap_or_else(|| {
        panic!("expected an error for `o1`, got:\n{out}");
    });
    let o2 = out.find("`o2`").unwrap_or_else(|| {
        panic!("expected an error for `o2` — before #750 only the first was reported:\n{out}");
    });
    assert!(
        o1 < o2,
        "errors must follow source order (`o1` on line 7 before `o2` on line 8):\n{out}"
    );
    assert!(
        out.contains("2 errors"),
        "batch should be summarised with a count:\n{out}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_single_error_is_not_wrapped_in_a_batch() {
    // The common case must render exactly as it did before batching — no
    // "1 error" header above a lone diagnostic.
    let dir = workdir("single");
    std::fs::write(
        dir.join("One.arch"),
        "\
module One
  port a: in UInt<8>;
  port b: in UInt<8>;
  port o1: out UInt<8>;
  comb
    o1 = a + b;
  end comb
end module One
",
    )
    .expect("write");
    let out = check_output(&dir, &["One.arch"]);

    assert!(out.contains("width mismatch"), "expected the error:\n{out}");
    assert!(
        !out.contains("1 error"),
        "a single error must not gain a batch wrapper:\n{out}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn batched_output_is_byte_identical_across_runs() {
    // Passes push errors in visit order, which is neither source order nor
    // guaranteed stable. Sorting by span offset is what makes the report
    // predictable to a human fixing top-to-bottom and to an agent diffing
    // successive runs. Same failure class as #756.
    let dir = workdir("determinism");
    std::fs::write(dir.join("Two.arch"), TWO_ERRORS).expect("write");
    let first = check_output(&dir, &["Two.arch"]);
    for run in 2..=4 {
        let again = check_output(&dir, &["Two.arch"]);
        assert_eq!(first, again, "run {run} differed from run 1");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn errors_from_several_files_render_against_their_own_source() {
    // A `Report` carries one `source_code`, so a batch spanning inputs needs
    // the snippet attached per error rather than per batch. If that is wrong
    // the line numbers silently point into the wrong file.
    let dir = workdir("multifile");
    std::fs::write(
        dir.join("A.arch"),
        "\
module A
  port a: in UInt<8>;
  port oa: out UInt<8>;
  comb
    oa = a + a;
  end comb
end module A
",
    )
    .expect("write A");
    std::fs::write(
        dir.join("B.arch"),
        "\
module B
  port b: in UInt<8>;
  port ob: out UInt<8>;
  comb
    ob = b + b;
  end comb
end module B
",
    )
    .expect("write B");

    let out = check_output(&dir, &["A.arch", "B.arch"]);
    assert!(out.contains("2 errors"), "both files should report:\n{out}");
    // Each snippet must be attributed to its own file, at that file's line 5.
    assert!(
        out.contains("A.arch:5:5"),
        "A's error should point into A.arch:\n{out}"
    );
    assert!(
        out.contains("B.arch:5:5"),
        "B's error should point into B.arch:\n{out}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_pathological_batch_is_capped_and_says_so() {
    // Past a screenful the list stops being actionable, but a truncated batch
    // must never read as complete.
    let dir = workdir("cap");
    let mut src = String::from("module Many\n  port a: in UInt<8>;\n  port b: in UInt<8>;\n");
    for i in 0..60 {
        src.push_str(&format!("  port o{i}: out UInt<8>;\n"));
    }
    src.push_str("  comb\n");
    for i in 0..60 {
        src.push_str(&format!("    o{i} = a + b;\n"));
    }
    src.push_str("  end comb\nend module Many\n");
    std::fs::write(dir.join("Many.arch"), src).expect("write");

    let out = check_output(&dir, &["Many.arch"]);
    assert!(
        out.contains("50 errors shown, 10 more not listed"),
        "expected a capped summary naming the remainder:\n{}",
        &out[..out.len().min(400)]
    );
    let _ = std::fs::remove_dir_all(&dir);
}
