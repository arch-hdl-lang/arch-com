mod build_version;

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn collect_files(dir: &Path, python_root: &Path, files: &mut Vec<String>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            if entry.file_name() != "__pycache__" {
                collect_files(&path, python_root, files)?;
            }
        } else if file_type.is_file() && path.extension().is_some_and(|ext| ext == "py") {
            let relative = path
                .strip_prefix(python_root)
                .expect("embedded Python file must be below python/");
            files.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

fn graph_generator_id(root: &Path) -> io::Result<String> {
    fn collect_rust(dir: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_dir() {
                collect_rust(&path, files)?;
            } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
        Ok(())
    }
    let mut files = vec![root.join("Cargo.lock"), root.join("build.rs")];
    collect_rust(&root.join("src"), &mut files)?;
    files.sort();
    let mut hash = 0xcbf29ce484222325u64;
    for path in files {
        let relative = path
            .strip_prefix(root)
            .expect("graph source inside checkout");
        for byte in relative
            .to_string_lossy()
            .as_bytes()
            .iter()
            .copied()
            .chain([0])
            .chain(fs::read(&path)?.into_iter().chain([0]))
        {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    Ok(format!("{hash:016x}"))
}

fn main() -> io::Result<()> {
    println!("cargo:rerun-if-changed=python/arch_cocotb");
    println!("cargo:rerun-if-changed=python/cocotb_shim/cocotb");

    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set"));
    println!("cargo:rerun-if-changed=build_version.rs");
    for path in build_version::watch_paths(&manifest_dir) {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    println!(
        "cargo:rustc-env=ARCH_BUILD_VERSION={}",
        build_version::version(&manifest_dir, &env::var("CARGO_PKG_VERSION").unwrap())
    );
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-changed=build.rs");
    println!(
        "cargo:rustc-env=ARCH_GRAPH_GENERATOR_ID={}",
        graph_generator_id(&manifest_dir)?
    );
    let python_root = manifest_dir.join("python");
    let mut files = Vec::new();
    collect_files(&python_root.join("arch_cocotb"), &python_root, &mut files)?;
    collect_files(
        &python_root.join("cocotb_shim").join("cocotb"),
        &python_root,
        &mut files,
    )?;
    files.sort();

    let mut generated = String::from("const EMBEDDED_PYTHON_FILES: &[(&str, &[u8])] = &[\n");
    for relative in files {
        generated.push_str(&format!(
            "    ({relative:?}, include_bytes!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \
             \"/python/{relative}\"))),\n"
        ));
    }
    generated.push_str("];\n");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR must be set"));
    fs::write(out_dir.join("embedded_python_files.rs"), generated)
}
