//! Regression coverage for wide reduction operators in native simulation.
//!
//! Values from 65 through 128 bits use `_arch_u128` internally.  The
//! reduction AND and XOR emitters must inspect the upper half of that value,
//! while the neighbouring 64-bit and VlWide paths retain their behavior.

use std::fs;
use std::process::Command;

fn arch() -> Command {
    Command::new(env!("CARGO_BIN_EXE_arch"))
}

#[test]
fn wide_reductions_cover_storage_boundaries() {
    let td = tempfile::tempdir().expect("tempdir");
    for width in [64u32, 65, 127, 128, 129] {
        let design = td.path().join(format!("reduce_{width}.arch"));
        let tb = td.path().join(format!("reduce_{width}_tb.cpp"));
        let outdir = td.path().join(format!("out_{width}"));
        fs::write(
            &design,
            format!(
                "module ReduceWide{width}\n  port data: in UInt<{width}>;\n  port all_ones: out Bool;\n  port parity: out Bool;\n  comb\n    all_ones = &data;\n    parity = ^data;\n  end comb\nend module ReduceWide{width}\n"
            ),
        )
        .expect("write design");

        let words = (width + 31) / 32;
        let assignments = if width <= 64 {
            String::from("d.data = value;\n")
        } else {
            (0..words)
                .map(|i| format!("d.data._data[{i}] = words[{i}];\n"))
                .collect()
        };
        let clear = if width <= 64 {
            String::from("d.data = 0;\n")
        } else {
            (0..words)
                .map(|i| format!("d.data._data[{i}] = 0;\n"))
                .collect()
        };
        let cases = if width <= 64 {
            format!(
                "uint64_t value = 0xffffffffffffffffULL;\n{assignments}d.eval(); if (!d.all_ones || ({parity}) != d.parity) return 1;\nvalue = 0xffffffffULL;\n{assignments}d.eval(); if (d.all_ones || d.parity != 0) return 2;\nvalue = (uint64_t(1) << {upper});\n{assignments}d.eval(); if (d.all_ones || d.parity != 1) return 3;\n",
                parity = if width % 2 == 0 { 0 } else { 1 },
                upper = width - 1,
                assignments = assignments,
            )
        } else {
            let allones_words = (0..words)
                .map(|i| {
                    let bits = (width - i * 32).min(32);
                    format!(
                        "words[{i}] = 0x{:x};\n",
                        if bits == 32 {
                            u32::MAX
                        } else {
                            (1u32 << bits) - 1
                        }
                    )
                })
                .collect::<String>();
            let lowones_words = (0..words)
                .map(|i| format!("words[{i}] = {};\n", if i < 2 { "0xffffffff" } else { "0" }))
                .collect::<String>();
            let upper_word = (width - 1) / 32;
            let upper_bit = (width - 1) % 32;
            let upper_words = (0..words)
                .map(|i| {
                    format!(
                        "words[{i}] = {};\n",
                        if i == upper_word {
                            format!("(1u << {upper_bit})")
                        } else {
                            "0".to_string()
                        }
                    )
                })
                .collect::<String>();
            format!(
                "uint32_t words[{words}] = {{}};\n{allones_words}{assignments}d.eval(); if (!d.all_ones || d.parity != {all_parity}) return 1;\n{clear}{lowones_words}{assignments}d.eval(); if (d.all_ones || d.parity != 0) return 2;\n{clear}{upper_words}{assignments}d.eval(); if (d.all_ones || d.parity != 1) return 3;\n",
                all_parity = width % 2,
                words = words,
                allones_words = allones_words,
                lowones_words = lowones_words,
                upper_words = upper_words,
                assignments = assignments,
                clear = clear,
            )
        };
        fs::write(
            &tb,
            format!(
                "#include \"VReduceWide{width}.h\"\n#include <cstdint>\nint main() {{ VReduceWide{width} d; {cases} return 0; }}\n",
                width = width,
                cases = cases,
            ),
        )
        .expect("write testbench");

        let result = arch()
            .arg("sim")
            .arg(&design)
            .arg("--tb")
            .arg(&tb)
            .arg("--outdir")
            .arg(&outdir)
            .output()
            .expect("run arch sim");
        assert!(
            result.status.success(),
            "wide reduction width {width} failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
}
