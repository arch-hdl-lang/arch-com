use std::path::Path;
use std::process::Command;

fn write_fixture(root: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let a = root.join("a");
    let b = root.join("b");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();

    let main = a.join("Main.arch");
    let consumer = b.join("Consumer.arch");
    let package = b.join("Pkg.arch");
    std::fs::write(&main, "module Main\n  port a: in Bool;\nend module Main\n").unwrap();
    std::fs::write(
        &consumer,
        "use Pkg;\n\nmodule Consumer\n  port word: in Word;\nend module Consumer\n",
    )
    .unwrap();
    std::fs::write(
        &package,
        "package Pkg\n  struct Word\n    data: UInt<8>;\n  end struct Word\nend package Pkg\n",
    )
    .unwrap();

    (main, consumer)
}

fn check(args: &[&Path]) -> std::process::Output {
    let root = args[0].parent().unwrap().parent().unwrap();
    Command::new(env!("CARGO_BIN_EXE_arch"))
        .arg("check")
        .env("ARCH_NO_STDLIB", "1")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap()
}

#[test]
fn imports_resolve_relative_to_each_input_file() {
    let td = tempfile::tempdir().unwrap();
    let (main, consumer) = write_fixture(td.path());
    let package = td.path().join("b/Pkg.arch");

    let standalone = check(&[&consumer]);
    assert!(
        standalone.status.success(),
        "standalone consumer failed:\n{}",
        String::from_utf8_lossy(&standalone.stderr)
    );

    for inputs in [
        vec![main.as_path(), consumer.as_path(), package.as_path()],
        vec![consumer.as_path(), main.as_path(), package.as_path()],
    ] {
        let result = check(&inputs);
        assert!(
            result.status.success(),
            "multi-file check failed for {:?}:\n{}",
            inputs,
            String::from_utf8_lossy(&result.stderr)
        );
    }
}
