//! Coverage for #797 — `inst` connections must name a real port of the child.

use arch::elaborate;
use arch::lexer;
use arch::parser::Parser;
use arch::resolve;
use arch::typecheck::TypeChecker;
use std::process::Command;

fn errors_from_source(source: &str) -> Vec<String> {
    let tokens = lexer::tokenize(source).expect("lex");
    let mut parser = Parser::new(tokens, source);
    let ast = parser.parse_source_file().expect("parse");
    let ast = elaborate::elaborate(ast).expect("elaborate");
    let symbols = resolve::resolve(&ast).expect("resolve");
    let checker = TypeChecker::new(&symbols, &ast);
    match checker.check() {
        Ok(_) => vec![],
        Err(errs) => errs.into_iter().map(|e| e.to_string()).collect(),
    }
}

fn check_ok(source: &str) {
    let errs = errors_from_source(source);
    assert!(errs.is_empty(), "expected OK, got errors: {:?}", errs);
}

fn check_err_contains(source: &str, substrs: &[&str]) {
    let errs = errors_from_source(source);
    assert!(!errs.is_empty(), "expected errors but got OK");
    let all = errs.join("\n");
    for s in substrs {
        assert!(all.contains(s), "expected '{}' in errors: {:?}", s, errs);
    }
}

#[test]
fn unknown_scalar_port_is_error() {
    let src = r#"
        module Child
          port d: in UInt<8>;
          port q: out UInt<8>;
          comb q = d; end comb
        end module Child
        module Top
          port a: in UInt<8>;
          port y: out UInt<8>;
          wire w: UInt<8>;
          inst c: Child
            d <- a;
            q -> w;
            this_port_does_not_exist <- a;
          end inst c
          comb y = w; end comb
        end module Top
    "#;
    let errs = errors_from_source(src);
    assert!(
        errs.iter()
            .any(|e| e.contains("this_port_does_not_exist") && e.contains("not a port of")),
        "bad pin not reported: {:?}",
        errs
    );
    // No suggestion for distant name (distance 24 >3)
    assert!(
        !errs
            .iter()
            .any(|e| e.contains("this_port_does_not_exist") && e.contains("did you mean")),
        "should not suggest for distant name: {:?}",
        errs
    );
}

#[test]
fn did_you_mean_suggests_closest_port() {
    let src = r#"
        module Child2
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync, High>;
          port d: in UInt<8>;
          port q: out UInt<8>;
          comb q = d; end comb
        end module Child2
        module Top2
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync, High>;
          port a: in UInt<8>;
          port y: out UInt<8>;
          wire w: UInt<8>;
          inst c: Child2
            clk <- clk;
            rst <- rst;
            d <- a;
            clok <- a;
          end inst c
          comb y = w; end comb
        end module Top2
    "#;
    check_err_contains(src, &["clok", "did you mean `clk`"]);
}

#[test]
fn did_you_mean_is_deterministic() {
    let src = r#"
        module Child
          port abc: in UInt<8>;
          port abd: in UInt<8>;
          port q: out UInt<8>;
          comb q = abc + abd; end comb
        end module Child
        module Top
          port a: in UInt<8>;
          port y: out UInt<8>;
          wire w: UInt<8>;
          inst c: Child
            abe <- a;
            q -> w;
          end inst c
          comb y = w; end comb
        end module Top
    "#;
    let errs1 = errors_from_source(src);
    let errs2 = errors_from_source(src);
    assert_eq!(errs1, errs2, "determinism failed");
    let all = errs1.join("\n");
    // abe distance 1 to both abc and abd, should pick lexicographically smallest abc
    assert!(
        all.contains("did you mean `abc`"),
        "expected deterministic suggestion abc, got: {:?}",
        errs1
    );
}

#[test]
fn multiple_unknown_ports_all_reported() {
    let src = r#"
        module Child
          port d: in UInt<8>;
          port q: out UInt<8>;
          comb q = d; end comb
        end module Child
        module Top
          port a: in UInt<8>;
          port y: out UInt<8>;
          wire w: UInt<8>;
          inst c: Child
            bad1 <- a;
            bad2 <- a;
            bad3 <- a;
          end inst c
          comb y = w; end comb
        end module Top
    "#;
    let errs = errors_from_source(src);
    // Should have 3 bad pins + 1 unconnected d (since d not connected) = 4, but at least 3 bad
    assert!(
        errs.iter().filter(|e| e.contains("not a port of")).count() == 3,
        "expected 3 bad pin errors, got: {:?}",
        errs
    );
}

#[test]
fn bus_per_field_typo_is_error() {
    let src = r#"
        bus MyBus
          cmd: out UInt<8>;
          resp: in UInt<8>;
        end bus MyBus
        module ChildB
          port p: target MyBus;
          comb p.resp = p.cmd; end comb
        end module ChildB
        module ParentB
          port a: in UInt<8>;
          port b: out UInt<8>;
          wire w: UInt<8>;
          inst c: ChildB
            p.cmd <- a;
            p.fake <- a;
          end inst c
          comb b = w; end comb
        end module ParentB
    "#;
    let errs = errors_from_source(src);
    assert!(
        errs.iter()
            .any(|e| e.contains("p_fake") && e.contains("not a port of")),
        "bus fake not reported: {:?}",
        errs
    );
}

#[test]
fn bus_handshake_per_field_still_valid() {
    let src = r#"
        bus HsBus
          handshake ch: send kind: valid_ready
            len: UInt<8>;
            data: UInt<32>;
          end handshake ch
        end bus HsBus
        module ChildHs
          port p: target HsBus;
          comb p.ch_valid = false; p.ch_data = 32'h00; end comb
        end module ChildHs
        module ParentHs
          port a: in UInt<8>;
          port b: in UInt<32>;
          wire w_valid: Bool;
          inst c: ChildHs
            p.ch_valid <- w_valid;
            p.ch_len <- a;
            p.ch_data <- b;
          end inst c
        end module ParentHs
    "#;
    // Should be OK (handshake bus permissive, but valid fields)
    // Note we use p.ch_len vs len? The handshake payload fields are ch_len, ch_data
    // The bus signal names are ch_len etc.
    // Use the correct flattened names via bus handshake: p.ch_valid etc.
    // This should not error for valid fields.
    let errs = errors_from_source(src);
    // Filter out unrelated errors (like unconnected warnings are not errors)
    let not_port_errs: Vec<_> = errs
        .iter()
        .filter(|e| e.contains("not a port of"))
        .collect();
    assert!(
        not_port_errs.is_empty(),
        "handshake valid fields should not error, got: {:?}",
        errs
    );
}

#[test]
fn vec_bus_whole_and_per_element_still_valid() {
    let src = r#"
        bus MyBus
          valid: out Bool;
          data: out UInt<8>;
        end bus MyBus
        module ChildVec
          port mm: target Vec<MyBus, 2>;
          comb
            mm[0].valid = true; mm[0].data = 8'h00;
            mm[1].valid = true; mm[1].data = 8'h01;
          end comb
        end module ChildVec
        module TopVec
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync, High>;
          wire w: MyBus;
          wire w2: MyBus;
          inst c: ChildVec
            mm_0 <- w;
            mm_1 <- w2;
          end inst c
        end module TopVec
    "#;
    // mm_0 and mm_1 are per-element whole-bus connections, should be valid
    // We use mm_0 syntax directly (parser flattens chans[0] -> chans_0)
    check_ok(src);
}

#[test]
fn vec_bus_per_element_signal_with_underscores() {
    // mm_0_cmd_valid must be valid — guards splitn(2, '_')
    let src = r#"
        bus MyBus
          cmd_valid: out Bool;
          cmd_data: out UInt<8>;
        end bus MyBus
        module ChildVec2
          port mm: target Vec<MyBus, 2>;
          comb
            mm[0].cmd_valid = true; mm[0].cmd_data = 8'h00;
            mm[1].cmd_valid = true; mm[1].cmd_data = 8'h01;
          end comb
        end module ChildVec2
        module TopVec2
          port a: in Bool;
          port b: in UInt<8>;
          inst c: ChildVec2
            mm_0_cmd_valid <- a;
            mm_0_cmd_data <- b;
            mm_1_cmd_valid <- a;
          end inst c
        end module TopVec2
    "#;
    check_ok(src);
    // Typo on that signal should error
    let src_bad = r#"
        bus MyBus
          cmd_valid: out Bool;
          cmd_data: out UInt<8>;
        end bus MyBus
        module ChildVec2
          port mm: target Vec<MyBus, 2>;
          comb
            mm[0].cmd_valid = true; mm[0].cmd_data = 8'h00;
          end comb
        end module ChildVec2
        module TopVec2
          port a: in Bool;
          inst c: ChildVec2
            mm_0_cmd_fake <- a;
          end inst c
        end module TopVec2
    "#;
    let errs = errors_from_source(src_bad);
    assert!(
        errs.iter()
            .any(|e| e.contains("mm_0_cmd_fake") && e.contains("not a port of")),
        "vec bus signal typo not reported: {:?}",
        errs
    );
}

#[test]
fn vec_data_per_element_valid() {
    let src_ok = r#"
        module Child
          port data: out Vec<UInt<8>, 4>;
          comb data[0]=8'h00; data[1]=8'h01; data[2]=8'h02; data[3]=8'h03; end comb
        end module Child
        module Top
          port y: out UInt<8>;
          wire w: UInt<8>;
          inst c: Child
            data_0 -> w;
          end inst c
          comb y = w; end comb
        end module Top
    "#;
    // data_0 is valid per-element
    let errs = errors_from_source(src_ok);
    let not_port = errs.iter().filter(|e| e.contains("not a port of")).count();
    assert_eq!(not_port, 0, "data_0 should be valid, got: {:?}", errs);

    let src_bad = r#"
        module Child
          port data: out Vec<UInt<8>, 4>;
          comb data[0]=8'h00; end comb
        end module Child
        module Top
          port y: out UInt<8>;
          wire w: UInt<8>;
          inst c: Child
            data_typo -> w;
          end inst c
          comb y = w; end comb
        end module Top
    "#;
    check_err_contains(src_bad, &["data_typo", "not a port of"]);
}

#[test]
fn generate_for_inst_unknown_port() {
    let src = r#"
        module Child
          port d: in UInt<8>;
          port q: out UInt<8>;
          comb q = d; end comb
        end module Child
        module Top
          param N: const = 2;
          port a: in UInt<8>;
          port y: out UInt<8>;
          wire w: UInt<8>;
          generate_for i in 0..N-1
            inst c_i: Child
              d <- a;
              bad <- a;
            end inst c_i
          end generate_for
          comb y = w; end comb
        end module Top
    "#;
    let errs = errors_from_source(src);
    assert!(
        errs.iter()
            .any(|e| e.contains("bad") && e.contains("not a port of")),
        "generate bad pin not reported: {:?}",
        errs
    );
}

#[test]
fn pipeline_child_unknown_port() {
    let src = r#"
        pipeline MyPipe
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync, High>;
          port d: in UInt<8>;
          port q: out UInt<8>;
          stage S0
            comb q = d; end comb
          end stage S0
        end pipeline MyPipe
        module Top
          port clk: in Clock<SysDomain>;
          port rst: in Reset<Sync, High>;
          port a: in UInt<8>;
          port y: out UInt<8>;
          wire w: UInt<8>;
          inst c: MyPipe
            clk <- clk;
            rst <- rst;
            d <- a;
            bad <- a;
          end inst c
          comb y = w; end comb
        end module Top
    "#;
    check_err_contains(src, &["bad", "not a port of", "MyPipe"]);
}

#[test]
fn unresolvable_child_kind_is_skipped_not_flagged() {
    // synchronizer has no ConstructCommon ports to validate against; even a bogus pin should not be flagged via not-a-port
    // It should instead be validated via other means or skipped. Our child_ports returns Some for synchronizer? Actually synchronizer has common, so it has ports, but we skip bus/sync/clkGate/template in child_ports? No, we return Some for synchronizer via common, so it would be validated. The plan says synchronizer should be skipped.
    // The plan's test says synchronizer or clkgate should be skipped (no port list to validate). Let's test synchronizer: it has ports clk_in, clk_out etc, but our child_ports returns Some for it, so bogus pin would be flagged. The plan's test expects zero errors for synchronizer bogus pin.
    // However our implementation now includes synchronizer? Let's check: child_ports returns Some for synchronizer? In our code, we match Item::Synchronizer? No, we only handle Module/Fsm/Pipeline/Fifo/Ram/Cam/Counter/Arbiter/Regfile/Linklist. We do NOT handle Synchronizer/Clkgate/Bus/Template. So synchronizer returns None, so validate skips. Good.
    let src = r#"
        synchronizer MySync
          port clk_in: in Clock<SysDomain>;
          port clk_out: in Clock<OtherDomain>;
          port d: in Bool;
          port q: out Bool;
        end synchronizer MySync
        module Top
          port clk1: in Clock<SysDomain>;
          port clk2: in Clock<OtherDomain>;
          port a: in Bool;
          port y: out Bool;
          wire w: Bool;
          inst s: MySync
            clk_in <- clk1;
            clk_out <- clk2;
            d <- a;
            bogus <- a;
            q -> w;
          end inst s
          comb y = w; end comb
        end module Top
    "#;
    let errs = errors_from_source(src);
    // Our child_ports for synchronizer returns None (since not in match), so bogus should NOT be flagged as not-a-port
    // It may still be flagged as undefined? But our logic skips, so zero not-a-port errors
    let not_port = errs.iter().filter(|e| e.contains("not a port of")).count();
    assert_eq!(
        not_port, 0,
        "synchronizer bogus should be skipped, got: {:?}",
        errs
    );
}

#[test]
fn vec_data_element_index_out_of_range_is_error() {
    // `data_9` on a `Vec<UInt<8>, 4>` names no port. Nothing downstream catches
    // it — before this check it reached Verilator as a bogus pin — so the
    // element index is bounds-checked here whenever the count is const.
    let src = r#"
        module Leaf
          port data: out Vec<UInt<8>, 4>;
          comb
            for i in 0..3
              data[i] = 0;
            end for
          end comb
        end module Leaf
        module Top
          port y: out UInt<8>;
          wire w: UInt<8>;
          inst c: Leaf
            data_9 -> w;
          end inst c
          comb y = w; end comb
        end module Top
    "#;
    check_err_contains(src, &["data_9", "not a port of"]);
}

#[test]
fn vec_bus_element_index_out_of_range_is_error() {
    // Same rule as the scalar-Vec case: `mm_9` on a `Vec<MyBus, 2>` names no
    // port. Kept in step with `vec_data_element_index_out_of_range_is_error` so
    // the two Vec flavours cannot drift apart.
    let src = r#"
        bus MyBus
          valid: out Bool;
          data: out UInt<8>;
        end bus MyBus
        module ChildVec
          port mm: target Vec<MyBus, 2>;
          comb
            mm[0].valid = true; mm[0].data = 8'h00;
            mm[1].valid = true; mm[1].data = 8'h01;
          end comb
        end module ChildVec
        module TopVec
          wire w: MyBus;
          inst c: ChildVec
            mm_9 <- w;
          end inst c
        end module TopVec
    "#;
    check_err_contains(src, &["mm_9", "not a port of"]);
}

/// The `.archi` separate-compilation path, end to end through the CLI.
///
/// `.archi` discovery lives in `src/main.rs`, so an in-process test cannot
/// reach it — it would silently exercise the in-source lookup instead and
/// prove nothing. This drives the real binary with a stub that is reachable
/// *only* via `ARCH_LIB_PATH`, which is the route the e203 corpus depends on.
#[test]
fn archi_child_via_arch_lib_path_unknown_port() {
    // Per-process directory: the path is otherwise global, and concurrent
    // `cargo test` runs on one machine would race on the create/remove.
    let tmp = std::env::temp_dir().join(format!("arch_issue797_archi_{}", std::process::id()));
    let lib = tmp.join("lib");
    let work = tmp.join("work");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&lib).expect("mkdir lib");
    std::fs::create_dir_all(&work).expect("mkdir work");

    std::fs::write(
        lib.join("Leaf.archi"),
        "module Leaf\n  port d: in UInt<8>;\n  port q: out UInt<8>;\nend module Leaf\n",
    )
    .expect("write stub");

    // Every real port stays connected and the bogus pin is added alongside, so
    // the unknown-port error is the *only* diagnostic. Misspelling `d` outright
    // would also raise "input port `d` is not connected", and the CLI currently
    // displays just the first error (#750), masking what this test is for.
    let parent = |extra: &str| {
        format!(
            "module Top\n  port a: in UInt<8>;\n  port y: out UInt<8>;\n  wire w: UInt<8>;\n\
             \x20 inst c: Leaf\n    d <- a;\n    q -> w;\n{extra}  end inst c\n\
             \x20 comb\n    y = w;\n  end comb\nend module Top\n"
        )
    };

    let run = || {
        Command::new(env!("CARGO_BIN_EXE_arch"))
            .current_dir(&work)
            .env("ARCH_LIB_PATH", &lib)
            .args(["check", "Top.arch"])
            .output()
            .expect("failed to run `arch check`")
    };

    // Bogus pin against the ARCH_LIB_PATH-only stub.
    std::fs::write(work.join("Top.arch"), parent("    dd <- a;\n")).expect("write parent");
    let out = run();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !out.status.success(),
        "expected a check failure, got success: {combined}"
    );
    assert!(
        combined.contains("is not a port of `Leaf`") && combined.contains("did you mean `d`"),
        "expected the unknown-port error with a suggestion, got: {combined}"
    );

    // Negative twin: the same stub with no bogus pin must pass, so the test
    // cannot succeed merely because every `.archi` child errors.
    std::fs::write(work.join("Top.arch"), parent("")).expect("write parent");
    let out = run();
    assert!(
        out.status.success(),
        "correctly-spelled pin should check clean, got: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let _ = std::fs::remove_dir_all(&tmp);
}
