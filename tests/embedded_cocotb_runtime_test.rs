use std::fs;
use std::process::Command;

#[test]
fn copied_binary_runs_cocotb_test_from_embedded_runtime() {
    let pybind = Command::new("python3")
        .args(["-m", "pybind11", "--includes"])
        .output();
    if !matches!(pybind, Ok(output) if output.status.success()) {
        eprintln!("skipping embedded cocotb runtime test: python3/pybind11 unavailable");
        return;
    }

    let temp = tempfile::tempdir().expect("tempdir");
    let install_bin = temp.path().join("install").join("bin");
    let project = temp.path().join("project");
    let build_dir = project.join("sim-build");
    fs::create_dir_all(&install_bin).expect("create fake installation");
    fs::create_dir_all(&project).expect("create standalone project");

    // Copy the executable far enough away from target/{debug,release} that
    // checkout-relative Python discovery cannot succeed.
    let installed_arch = install_bin.join("arch-hdl");
    fs::copy(env!("CARGO_BIN_EXE_arch"), &installed_arch).expect("copy arch binary");

    let source_path = project.join("Probe.arch");
    fs::write(
        &source_path,
        r#"
module Probe
  port value: in UInt<8>;
  port echoed: out UInt<8>;
  let echoed = value;
end module Probe
"#,
    )
    .expect("write ARCH source");

    let test_path = project.join("test_probe.py");
    fs::write(
        &test_path,
        r#"
import cocotb
from cocotb.triggers import Timer


@cocotb.test()
async def test_embedded_runtime(dut):
    dut.value.value = 0x5A
    await Timer(1, units="ns")
    assert int(dut.echoed.value) == 0x5A
"#,
    )
    .expect("write cocotb test");

    let output = Command::new(&installed_arch)
        .current_dir(&project)
        .arg("sim")
        .arg("--pybind")
        .arg("--test")
        .arg(&test_path)
        .arg("--outdir")
        .arg(&build_dir)
        .arg(&source_path)
        .env_remove("ARCH_PYTHON_DIR")
        .output()
        .expect("run copied arch binary");

    assert!(
        output.status.success(),
        "copied binary should run without ARCH_PYTHON_DIR\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let embedded_root = build_dir
        .join("_arch_python")
        .join(env!("CARGO_PKG_VERSION"));
    assert!(
        embedded_root
            .join("arch_cocotb")
            .join("__init__.py")
            .is_file(),
        "arch_cocotb was not materialized below {}",
        embedded_root.display()
    );
    assert!(
        embedded_root
            .join("cocotb_shim")
            .join("cocotb")
            .join("__init__.py")
            .is_file(),
        "cocotb shim was not materialized below {}",
        embedded_root.display()
    );
}
