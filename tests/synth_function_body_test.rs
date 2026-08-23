//! Regression coverage for arch#932 — emitted `function automatic` bodies
//! must be parseable by yosys's built-in Verilog frontend.
//!
//! Two constructs made every arch-emitted function unsynthesizable, both
//! legal SystemVerilog that Verilator and Icarus accept, so no simulator
//! gate ever saw them:
//!
//!   * `logic [7:0] d = expr;` — declaration carrying an initializer inside
//!     a function body ("Invalid nesting of always blocks and/or
//!     initializations").
//!   * `return expr;` — rejected outright ("syntax error, unexpected
//!     TOK_ID").
//!
//! Declarations now hoist above the body and `return` becomes an assignment
//! to the function name — EXCEPT where a return is an early exit, which the
//! assignment form cannot express; those keep emitting a literal `return`.

use std::process::Command;

fn build_sv(src: &str, name: &str) -> String {
    let td = tempfile::tempdir().expect("tempdir");
    let arch_path = td.path().join(format!("{name}.arch"));
    let sv_path = td.path().join(format!("{name}.sv"));
    std::fs::write(&arch_path, src).expect("write arch");
    let out = Command::new(env!("CARGO_BIN_EXE_arch"))
        .arg("build")
        .arg(&arch_path)
        .arg("-o")
        .arg(&sv_path)
        .arg("--no-auto-asserts")
        .output()
        .expect("run arch build");
    assert!(
        out.status.success(),
        "arch build failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::read_to_string(&sv_path).expect("read sv")
}

/// The ordinary shape: declarations hoisted, `return` lowered to an
/// assignment to the function name.
#[test]
fn function_body_hoists_decls_and_assigns_return() {
    let sv = build_sv(
        "module SynthFnPlain\n\
         \x20 port a: in UInt<8>;\n\
         \x20 port o: out UInt<8>;\n\
         \x20 function dbl(x: UInt<8>) -> UInt<8>\n\
         \x20   let d: UInt<8> = x +% x;\n\
         \x20   return d;\n\
         \x20 end function dbl\n\
         \x20 comb\n\
         \x20   o = dbl(a);\n\
         \x20 end comb\n\
         end module SynthFnPlain\n",
        "SynthFnPlain",
    );
    assert!(
        sv.contains("logic [7:0] d;"),
        "declaration should be hoisted bare, got:\n{sv}"
    );
    assert!(
        !sv.contains("logic [7:0] d ="),
        "declaration must not carry an initializer:\n{sv}"
    );
    // The return became an assignment to the function's SV name.
    assert!(
        sv.contains("dbl = d;"),
        "return should lower to `<fn> = <expr>;`:\n{sv}"
    );
    assert!(
        !sv.contains("return d;"),
        "no literal `return` should survive here:\n{sv}"
    );
}

/// An if/else where every arm returns is still rewritable — the returns are
/// each terminal in their block and nothing follows the `if`.
#[test]
fn function_if_else_returns_are_rewritten() {
    let sv = build_sv(
        "module SynthFnIf\n\
         \x20 port a: in UInt<8>;\n\
         \x20 port o: out UInt<8>;\n\
         \x20 function pick(x: UInt<8>) -> UInt<8>\n\
         \x20   if x > 10\n\
         \x20     return x;\n\
         \x20   else\n\
         \x20     return 0;\n\
         \x20   end if\n\
         \x20 end function pick\n\
         \x20 comb\n\
         \x20   o = pick(a);\n\
         \x20 end comb\n\
         end module SynthFnIf\n",
        "SynthFnIf",
    );
    assert!(sv.contains("pick = x;"), "then-arm return:\n{sv}");
    assert!(!sv.contains("return x;"), "no literal return:\n{sv}");
}

/// THE GUARD. An early return — a returning `if` followed by another
/// statement — cannot become a bare assignment, because the trailing
/// statement would run and overwrite it. Such a function must keep its
/// literal `return`, even though yosys will not parse it: correctness wins
/// over synthesizability, and this is the status quo, not a regression.
///
/// Nothing in the corpus has this shape, so without this test the fallback
/// path would be entirely unexercised.
#[test]
fn early_return_keeps_literal_return() {
    let sv = build_sv(
        "module SynthFnEarly\n\
         \x20 port a: in UInt<8>;\n\
         \x20 port o: out UInt<8>;\n\
         \x20 function clamp_lo(x: UInt<8>) -> UInt<8>\n\
         \x20   if x < 4\n\
         \x20     return 4;\n\
         \x20   end if\n\
         \x20   return x;\n\
         \x20 end function clamp_lo\n\
         \x20 comb\n\
         \x20   o = clamp_lo(a);\n\
         \x20 end comb\n\
         end module SynthFnEarly\n",
        "SynthFnEarly",
    );
    assert!(
        sv.contains("return 4;") && sv.contains("return x;"),
        "early-exit function must keep literal returns so the early exit \
         survives; rewriting them would let `return x` overwrite the clamp:\n{sv}"
    );
    assert!(
        !sv.contains("clamp_lo = 4;"),
        "must NOT rewrite an early return:\n{sv}"
    );
}

/// Loop iterators must be declared with the function's other locals, never
/// inline in the `for` header (arch#932).
///
/// `for (int unsigned i = 0; ...)` nested inside an if/else arm leaves `i`
/// undriven on the sibling paths, and yosys's `proc` pass infers a LATCH per
/// bit-slice of the iterator — 18 of them in `ScaledQuant` alone. The emitted
/// logic is pure combinational quantization, so those latches are spurious
/// hardware that no simulator gate can see: Verilator and Icarus both accept
/// the inline form happily.
///
/// The placement matters as much as the hoist. A declaration emitted *after*
/// the function's first statement is illegal SV — yosys accepts it silently,
/// Verilator rejects it — so this also pins that every iterator declaration
/// precedes the first assignment in its function.
#[test]
fn block_helper_loop_iterators_are_declared_not_inline() {
    let td = tempfile::tempdir().expect("tempdir");
    let sv_path = td.path().join("ScaledQuant.sv");
    let out = Command::new(env!("CARGO_BIN_EXE_arch"))
        .arg("build")
        .arg("tests/fp_v1/ScaledQuant.arch")
        .arg("-o")
        .arg(&sv_path)
        .arg("--no-auto-asserts")
        .output()
        .expect("run arch build");
    assert!(
        out.status.success(),
        "arch build failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let sv = std::fs::read_to_string(&sv_path).expect("read sv");

    assert!(
        !sv.contains("for (int"),
        "loop iterator must not be declared inline in the `for` header — \
         nested in an if/else arm it yields a latch per bit-slice:\n{sv}"
    );
    assert!(
        sv.contains("int unsigned i;"),
        "iterator should be declared with the function's other locals:\n{sv}"
    );

    // Every iterator declaration must precede its function's first statement.
    for body in sv.split("function automatic").skip(1) {
        let body = body.split("endfunction").next().unwrap_or("");
        let Some(decl) = body.find("int unsigned i;") else {
            continue;
        };
        let first_stmt = body
            .lines()
            .scan(0usize, |off, l| {
                let at = *off;
                *off += l.len() + 1;
                Some((at, l.trim().to_string()))
            })
            .find(|(_, l)| {
                l.ends_with(';')
                    && !l.starts_with("logic")
                    && !l.starts_with("int ")
                    && !l.starts_with("//")
            })
            .map(|(at, _)| at);
        if let Some(stmt) = first_stmt {
            assert!(
                decl < stmt,
                "iterator declared after the first statement (illegal SV; \
                 yosys accepts it, Verilator does not):\n{body}"
            );
        }
    }
}
