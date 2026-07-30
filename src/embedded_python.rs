use std::fs;
use std::io;
use std::path::{Path, PathBuf};

include!(concat!(env!("OUT_DIR"), "/embedded_python_files.rs"));

const ARCH_PACKAGE_MARKER: &str = "arch_cocotb/__init__.py";
const COCOTB_PACKAGE_MARKER: &str = "cocotb_shim/cocotb/__init__.py";

pub(crate) fn is_complete_runtime(root: &Path) -> bool {
    root.join(ARCH_PACKAGE_MARKER).is_file() && root.join(COCOTB_PACKAGE_MARKER).is_file()
}

/// Materialize the Python runtime embedded in this exact compiler build.
///
/// The Cargo package version keeps side-by-side compiler installations from
/// sharing stale Python source when they use the same simulation output
/// directory.
pub(crate) fn materialize(build_dir: &Path) -> io::Result<PathBuf> {
    let runtime_root = build_dir
        .join("_arch_python")
        .join(env!("CARGO_PKG_VERSION"));

    for (relative, contents) in EMBEDDED_PYTHON_FILES {
        let destination = runtime_root.join(relative);
        if matches!(fs::read(&destination), Ok(existing) if existing == *contents) {
            continue;
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(destination, contents)?;
    }

    Ok(runtime_root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materializes_both_packages_and_repairs_changed_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = materialize(temp.path()).expect("materialize embedded Python");

        assert!(is_complete_runtime(&root));
        assert!(root.ends_with(env!("CARGO_PKG_VERSION")));

        let marker = root.join(ARCH_PACKAGE_MARKER);
        let embedded_marker = EMBEDDED_PYTHON_FILES
            .iter()
            .find(|(relative, _)| *relative == ARCH_PACKAGE_MARKER)
            .map(|(_, contents)| *contents)
            .expect("embedded arch_cocotb marker");
        fs::write(&marker, b"changed").expect("change materialized file");

        materialize(temp.path()).expect("repair embedded Python");
        assert_eq!(
            fs::read(marker).expect("read repaired file"),
            embedded_marker
        );
    }

    #[test]
    fn every_embedded_file_is_materialized() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = materialize(temp.path()).expect("materialize embedded Python");

        assert!(!EMBEDDED_PYTHON_FILES.is_empty());
        for (relative, contents) in EMBEDDED_PYTHON_FILES {
            assert_eq!(
                fs::read(root.join(relative)).expect("read materialized file"),
                *contents,
                "{relative} differs from its embedded contents"
            );
        }
    }
}
