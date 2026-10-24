use std::fs;
use std::process::Command;

fn arch() -> &'static str {
    env!("CARGO_BIN_EXE_arch")
}

#[test]
fn local_inst_provider_wins_over_library_provider() {
    let temp = tempfile::tempdir().expect("tempdir");
    let source = temp.path().join("src");
    let library = temp.path().join("lib");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&library).unwrap();
    fs::write(
        source.join("Top.arch"),
        "module Top\n  port a: in Bool;\n  inst u: Child\n    a <- a;\n  end inst u\nend module Top\n",
    )
    .unwrap();
    fs::write(
        source.join("Child.arch"),
        "module Child\n  port a: in Bool;\nend module Child\n",
    )
    .unwrap();
    fs::write(
        library.join("Child.arch"),
        "module Child\n  port b: in Bool;\nend module Child\n",
    )
    .unwrap();

    let local = Command::new(arch())
        .arg("check")
        .arg(source.join("Top.arch"))
        .env("ARCH_LIB_PATH", &library)
        .output()
        .unwrap();
    assert!(
        local.status.success(),
        "{}",
        String::from_utf8_lossy(&local.stderr)
    );

    fs::remove_file(source.join("Child.arch")).unwrap();
    let library_only = Command::new(arch())
        .arg("check")
        .arg(source.join("Top.arch"))
        .env("ARCH_LIB_PATH", &library)
        .output()
        .unwrap();
    assert!(
        !library_only.status.success(),
        "library provider should expose the distinct interface"
    );
    // Prove library-only resolution succeeds with its actual interface.
    fs::write(source.join("Top.arch"), "module Top\n  port a: in Bool;\n  inst u: Child\n    b <- a;\n  end inst u\nend module Top\n").unwrap();
    let library_valid = Command::new(arch())
        .arg("check")
        .arg(source.join("Top.arch"))
        .env("ARCH_LIB_PATH", &library)
        .output()
        .unwrap();
    assert!(
        library_valid.status.success(),
        "{}",
        String::from_utf8_lossy(&library_valid.stderr)
    );

    // Explicit use must select source, even when a stale local interface exists.
    fs::write(
        source.join("Child.archi"),
        "module Child\n  port stale: in Bool;\nend module Child\n",
    )
    .unwrap();
    fs::write(source.join("Top.arch"), "use Child;\nmodule Top\n  port a: in Bool;\n  inst u: Child\n    b <- a;\n  end inst u\nend module Top\n").unwrap();
    let explicit = Command::new(arch())
        .arg("check")
        .arg(source.join("Top.arch"))
        .env("ARCH_LIB_PATH", &library)
        .output()
        .unwrap();
    assert!(
        explicit.status.success(),
        "{}",
        String::from_utf8_lossy(&explicit.stderr)
    );
}

#[test]
fn explicit_provider_is_stable_regardless_of_input_order() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::write(
        temp.path().join("Provider.arch"),
        "module Child\n  port a: in Bool;\nend module Child\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("Child.arch"),
        "module Child\n  port stale: in Bool;\nend module Child\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("Top.arch"),
        "use Provider;\nmodule Top\n  port a: in Bool;\n  inst u: Child\n    a <- a;\n  end inst u\nend module Top\n",
    )
    .unwrap();

    for inputs in [
        vec!["Provider.arch", "Top.arch"],
        vec!["Top.arch", "Provider.arch"],
    ] {
        let output = Command::new(arch())
            .arg("check")
            .args(inputs)
            .current_dir(temp.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let standalone = Command::new(arch())
        .args(["check", "Provider.arch"])
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert!(standalone.status.success());
}
