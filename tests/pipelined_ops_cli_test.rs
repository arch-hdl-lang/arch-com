//! CLI-level coverage for `arch ops` (doc/proposal_pipelined_operators.md
//! phase 1). Verifies the plain-text listing, the `--markdown` listing, and
//! that the checked-in doc/generated/pipelined_ops.md has not drifted from
//! what the compiler currently emits.

use std::process::Command;

fn run_ops(extra_args: &[&str]) -> String {
    let arch_bin = env!("CARGO_BIN_EXE_arch");
    let out = Command::new(arch_bin)
        .arg("ops")
        .args(extra_args)
        .output()
        .expect("failed to run `arch ops`");
    assert!(
        out.status.success(),
        "arch ops {:?} failed: stderr={}",
        extra_args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("arch ops stdout is not utf8")
}

#[test]
fn ops_text_listing_snapshot() {
    let stdout = run_ops(&[]);
    let mut lines = stdout.lines();
    let header = lines.next().expect("header line");
    assert_eq!(
        header.split_whitespace().collect::<Vec<_>>(),
        vec![
            "operator",
            "profile",
            "stages",
            "status",
            "fmax(ng45,typ)",
            "impl"
        ]
    );
    let row = lines.next().expect("data row");
    let cells: Vec<&str> = row.split_whitespace().collect();
    assert_eq!(cells[0], "fma");
    assert_eq!(cells[1], "FP32");
    assert_eq!(cells[2], "6");
    assert_eq!(cells[3], "verified");
    assert_eq!(cells[4], "~260");
    assert_eq!(cells[5], "MHz");
    // fmax cell is now an annotated "~260 MHz (external run — see notes)"
    // (proposal phase 3 — see src/pipelined_ops.rs registry entry notes for
    // why this repo's checked-in synth flow doesn't reproduce it), so the
    // `impl` column has shifted further right; only its value (the last
    // whitespace-split cell) is a stable assertion target.
    assert_eq!(cells.last().copied(), Some("builtin:fma_f32_s6"));
    assert!(stdout.contains("fma<FP32, 6>:"));
}

#[test]
fn ops_markdown_listing_matches_checked_in_doc() {
    let stdout = run_ops(&["--markdown"]);
    let checked_in = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/doc/generated/pipelined_ops.md"
    ))
    .expect(
        "doc/generated/pipelined_ops.md must exist — regenerate with \
         scripts/gen_pipelined_ops_doc.sh",
    );
    assert_eq!(
        stdout, checked_in,
        "doc/generated/pipelined_ops.md is stale — regenerate with \
         scripts/gen_pipelined_ops_doc.sh (arch ops --markdown > doc/generated/pipelined_ops.md)"
    );
}

#[test]
fn ops_markdown_listing_is_well_formed() {
    let stdout = run_ops(&["--markdown"]);
    assert!(stdout.starts_with("<!-- GENERATED FILE. DO NOT EDIT BY HAND."));
    assert!(stdout
        .contains("| operator | profile | stages | status | fmax (ng45, typ) | impl | notes |"));
    assert!(stdout.contains(
        "| `fma` | FP32 | 6 | verified | ~260 MHz (external run — see notes) | `builtin:fma_f32_s6` |"
    ));
}
