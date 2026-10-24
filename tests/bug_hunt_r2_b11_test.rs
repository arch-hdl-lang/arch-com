//! Regression coverage for native simulation of module-local functions.
//!
//! VFunctions.h is shared by every generated module, so local helpers with
//! the same source name must retain their owning-module identity.  This also
//! exercises calls from one local helper to another and declaration-order
//! independence.

use std::process::Command;

fn run_native(src: &str, tb: &str, name: &str) {
    if Command::new("c++").arg("--version").output().is_err() {
        eprintln!("skipping native simulation: c++ is unavailable");
        return;
    }
    let td = tempfile::tempdir().expect("tempdir");
    let arch_path = td.path().join(format!("{name}.arch"));
    let tb_path = td.path().join("tb.cpp");
    std::fs::write(&arch_path, src).expect("write arch source");
    std::fs::write(&tb_path, tb).expect("write testbench");
    let out = Command::new(env!("CARGO_BIN_EXE_arch"))
        .arg("sim")
        .arg(&arch_path)
        .arg("--tb")
        .arg(&tb_path)
        .arg("--outdir")
        .arg(td.path().join("out"))
        .output()
        .expect("run arch sim");
    assert!(
        out.status.success(),
        "native local-function simulation failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn source(left_first: bool) -> String {
    let left = "module Left\n port a: in UInt<8>;\n port y: out UInt<8>;\n function helper(x: UInt<8>) -> UInt<8>\n   return x +% 1;\n end function helper\n function transform(x: UInt<8>) -> UInt<8>\n   return helper(x);\n end function transform\n let y = transform(a);\nend module Left\n";
    let right = "module Right\n port a: in UInt<8>;\n port y: out UInt<8>;\n function helper(x: UInt<8>) -> UInt<8>\n   return x -% 1;\n end function helper\n function transform(x: UInt<8>) -> UInt<8>\n   return helper(x);\n end function transform\n let y = transform(a);\nend module Right\n";
    let modules = if left_first {
        format!("{left}{right}")
    } else {
        format!("{right}{left}")
    };
    format!(
        "{modules}module Top\n port a: in UInt<8>;\n port l: out UInt<8>;\n port r: out UInt<8>;\n inst left: Left\n a <- a;\n y -> l;\n end inst left\n inst right: Right\n a <- a;\n y -> r;\n end inst right\nend module Top\n"
    )
}

const TB: &str = r#"#include "VTop.h"
#include <cassert>
int main() { VTop t; t.a = 10; t.eval(); assert(t.l == 11); assert(t.r == 9); }
"#;

#[test]
fn local_helpers_keep_owner_identity_in_both_source_orders() {
    run_native(&source(true), TB, "local_functions_left_first");
    run_native(&source(false), TB, "local_functions_right_first");
}

#[test]
fn local_function_names_do_not_depend_on_signature_width() {
    let src = r#"
module Narrow
 port a: in UInt<8>;
 port y: out UInt<8>;
 function transform(x: UInt<8>) -> UInt<8>
   return x +% 1;
 end function transform
 let y = transform(a);
end module Narrow
module Wide
 port a: in UInt<16>;
 port y: out UInt<16>;
 function transform(x: UInt<16>) -> UInt<16>
   return x +% 2;
 end function transform
 let y = transform(a);
end module Wide
module Top
 port a8: in UInt<8>;
 port a16: in UInt<16>;
 port n: out UInt<8>;
 port w: out UInt<16>;
 inst narrow: Narrow
 a <- a8;
 y -> n;
 end inst narrow
 inst wide: Wide
 a <- a16;
 y -> w;
 end inst wide
end module Top
"#;
    let tb = r#"#include "VTop.h"
#include <cassert>
int main() { VTop t; t.a8 = 10; t.a16 = 1000; t.eval(); assert(t.n == 11); assert(t.w == 1002); }
"#;
    run_native(src, tb, "local_functions_widths");
}
