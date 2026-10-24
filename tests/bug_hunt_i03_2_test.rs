//! Regression for preserving derived ternary parameters in `.archi` files.

use std::fs;
use std::process::Command;

fn arch() -> &'static str {
    env!("CARGO_BIN_EXE_arch")
}

#[test]
fn archi_preserves_ternary_param_for_separate_compilation() {
    let tmp = std::env::temp_dir().join(format!("arch_bug_hunt_i03_2_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).expect("create temporary test directory");

    let width = r#"module Width
  param W: const = 8;
  param D: const = W > 4 ? 16 : 2;
  port a: in UInt<D>;
  port y: out UInt<D>;
  comb
    y = a;
  end comb
end module Width
"#;
    fs::write(tmp.join("Width.arch"), width).expect("write source");

    let built = Command::new(arch())
        .current_dir(&tmp)
        .args(["build", "Width.arch", "-o", "Width.sv"])
        .output()
        .expect("run arch build");
    assert!(
        built.status.success(),
        "build failed:\n{}{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );

    let archi = fs::read_to_string(tmp.join("Width.archi")).expect("read emitted interface");
    assert!(
        archi.contains("param D: const = ((W > 4) ? 16 : 2);"),
        "derived ternary must survive interface emission:\n{archi}"
    );
    // Ensure the following check exercises separate compilation rather than
    // rediscovering the original source beside the interface stub.
    fs::remove_file(tmp.join("Width.arch")).expect("remove source before consumer check");

    // The consumer sees only Width.archi.  Instantiate both parameter
    // branches to prove the preserved expression is still elaborated rather
    // than merely copied as text for the default specialization.
    let consumer = r#"module Top
  port a_hi: in UInt<16>;
  port y_hi: out UInt<16>;
  port a_lo: in UInt<2>;
  port y_lo: out UInt<2>;
  inst hi: Width
    param W = 8;
    a <- a_hi;
    y -> y_hi;
  end inst hi
  inst lo: Width
    param W = 4;
    a <- a_lo;
    y -> y_lo;
  end inst lo
end module Top
"#;
    fs::write(tmp.join("Top.arch"), consumer).expect("write consumer");
    let checked = Command::new(arch())
        .current_dir(&tmp)
        .args(["check", "Top.arch"])
        .output()
        .expect("run consumer check");
    assert!(
        checked.status.success(),
        "consumer check failed:\n{}{}",
        String::from_utf8_lossy(&checked.stdout),
        String::from_utf8_lossy(&checked.stderr)
    );

    let _ = fs::remove_dir_all(&tmp);
}
