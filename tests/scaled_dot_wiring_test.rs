//! `scaled_dot` wiring faithfulness — the composition half of Theorem A
//! (`proofs/lean_fp_equiv/SCALED_DOT_ACCUMULATION_SCOPE.md`).
//!
//! The renderer-faithfulness *SMT* miter (`tests/fp_v1/smt_proof/scaled_dot_miter.sh`)
//! proves the emitted SV equals the composed define-fun tree bit-for-bit, but it
//! only scales to small shapes: the widest-significand reduction tree
//! (`ScaledDotE4m3N4` and up) is SAT-hard — a monolithic solve times out, and
//! unlike FMA's single alignment gap a reduction tree has no single splitting
//! variable (measured: monolithic and top-add-gap splits both time out; a full
//! per-add-gap split is ~35^depth cases). So for wide/large shapes Theorem A is
//! discharged compositionally instead:
//!
//!   * **Node faithfulness** — each atomic node's SV equals its model — is
//!     machine-checked by `renderer_miter.sh` (`F32Mul`, `F32Add`, `E4m3ToF32`,
//!     `E5m2ToF32`, `E2m1ToF32`, `E3m2ToF32`, `E8m0ToF32` all `unsat`).
//!   * **Wiring faithfulness** — the emitted `arch_scaled_dot_*` function is
//!     *exactly* `dot_schedule`'s composition of those nodes — is what this test
//!     checks. SV functions are pure and context-free, so node-correct-in-
//!     isolation is node-correct-in-composition; the two halves compose to
//!     Theorem A for every shape, including the ones SMT cannot reach.
//!
//! The check is independent of `src/fp_block.rs`: it re-derives the expected
//! assignment sequence from the OCP balanced-pairwise schedule and the layout
//! convention, then compares the emitted function's assignment lines to it
//! *exactly* (order included) — so a wrong element index, wrong tree topology,
//! wrong scale association, or a spurious extra op all fail, not just pass-by-
//! substring. Changing `dot_schedule`'s order (e.g. to serial) fails this test
//! by design: it pins the defined accumulation order.

use std::process::Command;

/// Balanced-pairwise reduction schedule — an INDEPENDENT reimplementation of
/// `dot_schedule` (adjacent pairs each round; a lone trailing element passes
/// through untouched). Returns the adds as `(left, right)` temp indices (temp
/// `n + k` is `adds[k]`) and the final temp index.
fn balanced_pairwise(n: usize) -> (Vec<(usize, usize)>, usize) {
    let mut cur: Vec<usize> = (0..n).collect();
    let mut adds: Vec<(usize, usize)> = Vec::new();
    let mut next = n;
    while cur.len() > 1 {
        let mut nxt = Vec::new();
        let mut i = 0;
        while i + 1 < cur.len() {
            adds.push((cur[i], cur[i + 1]));
            nxt.push(next);
            next += 1;
            i += 2;
        }
        if i < cur.len() {
            nxt.push(cur[i]);
        }
        cur = nxt;
    }
    (adds, cur[0])
}

/// Build one `ScaledDot` module and return its emitted SystemVerilog.
fn build_scaled_dot(dir: &std::path::Path, elem_ty: &str, n: usize) -> String {
    let src = format!(
        "package P\n  type B = ScaledVec<{elem_ty}, {n}, E8M0>;\nend package P\n\n\
         module SdWire\n  port a: in B;\n  port b: in B;\n  port y: out FP32;\n\
         \x20 comb y = scaled_dot(a, b); end comb\nend module SdWire\n"
    );
    let arch = dir.join("SdWire.arch");
    let sv = dir.join("SdWire.sv");
    std::fs::write(&arch, src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_arch"))
        .args(["build", arch.to_str().unwrap(), "-o", sv.to_str().unwrap()])
        .output()
        .expect("run arch build");
    assert!(
        out.status.success(),
        "arch build failed for {elem_ty} N={n}:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::read_to_string(&sv).unwrap()
}

/// Pull the `arch_scaled_dot_*` function body's assignment lines (the `t.. = ..`
/// and final `name = ..` lines), trimmed, in order — skipping `logic` decls.
fn scaled_dot_assignments(sv: &str, fname: &str) -> Vec<String> {
    let start = sv
        .find(&format!("function automatic logic [31:0] {fname}("))
        .unwrap_or_else(|| panic!("function {fname} not found in emitted SV"));
    let body = &sv[start..];
    let end = body.find("\nendfunction").expect("endfunction");
    body[..end]
        .lines()
        .map(str::trim)
        .filter(|l| l.contains(" = ") && !l.starts_with("logic"))
        .map(str::to_string)
        .collect()
}

#[test]
fn scaled_dot_wiring_matches_balanced_pairwise_schedule() {
    // (arch tag, ARCH element type, element width in bits)
    let formats = [
        ("e4m3", "FP8E4M3", 8usize),
        ("e5m2", "FP8E5M2", 8),
        ("e2m1", "FP4E2M1", 4),
        ("e2m3", "FP6E2M3", 6),
        ("e3m2", "FP6E3M2", 6),
    ];
    // Include odd N (3, 5) to exercise the lone-element pass-through.
    let ns = [2usize, 3, 4, 5, 8];

    let tmp = std::env::temp_dir().join(format!("sd_wire_{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();

    for (tag, elem_ty, ew) in formats {
        for n in ns {
            let sv = build_scaled_dot(&tmp, elem_ty, n);
            let fname = format!("arch_scaled_dot_{tag}_{n}_e8m0");
            let got = scaled_dot_assignments(&sv, &fname);

            // Independently derive the expected assignment sequence.
            let bw = 8 + n * ew; // scale width (E8M0 = 8) + n elements
            let mut want: Vec<String> = Vec::new();
            // Products t0..t(n-1): element i at [i*ew +: ew].
            for i in 0..n {
                let lo = i * ew;
                want.push(format!(
                    "t{i} = arch_f32_mul(arch_{tag}_to_f32(a[{lo} +: {ew}]), \
                     arch_{tag}_to_f32(b[{lo} +: {ew}]));"
                ));
            }
            // Pairwise adds t(n)..; final temp index `last`.
            let (adds, last) = balanced_pairwise(n);
            for (k, (l, r)) in adds.iter().enumerate() {
                want.push(format!("t{} = arch_f32_add(t{l}, t{r});", n + k));
            }
            // Scale applied one at a time: ((sum * Xa) * Xb); scale at [bw-1 : bw-8].
            want.push(format!(
                "{fname} = arch_f32_mul(arch_f32_mul(t{last}, \
                 arch_e8m0_to_f32(a[{}:{}])), arch_e8m0_to_f32(b[{}:{}]));",
                bw - 1,
                bw - 8,
                bw - 1,
                bw - 8
            ));

            assert_eq!(
                got,
                want,
                "\nwiring mismatch for {elem_ty} N={n} ({fname}):\n\
                 emitted:\n  {}\nexpected (independent balanced-pairwise):\n  {}\n",
                got.join("\n  "),
                want.join("\n  ")
            );
        }
    }

    let _ = std::fs::remove_dir_all(&tmp);
}

/// The odd-element pass-through is the subtle part of the schedule: a lone
/// trailing element must ride to the next round untouched (never padded with a
/// `+0.0`, which would flip `-0.0`). Pin the exact N=3 shape as a focused guard.
#[test]
fn scaled_dot_odd_passthrough_shape() {
    let (adds, last) = balanced_pairwise(3);
    // round 1: add(0,1)->t3, lone 2 passes; round 2: add(t3,2)->t4.
    assert_eq!(adds, vec![(0, 1), (3, 2)]);
    assert_eq!(last, 4);

    let (adds5, last5) = balanced_pairwise(5);
    // [0,1,2,3,4]: add(0,1)=5, add(2,3)=6, lone 4; [5,6,4]: add(5,6)=7, lone 4;
    // [7,4]: add(7,4)=8.
    assert_eq!(adds5, vec![(0, 1), (2, 3), (5, 6), (7, 4)]);
    assert_eq!(last5, 8);
}
