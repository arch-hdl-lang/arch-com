//! Shared formal equation models for compiler-owned ARCH constructs.
//!
//! This is intentionally an ARCH-semantics IR, not an SV IR. Backends can
//! lower these equations to SMT-LIB2, Lean certificates, or future formal
//! targets without parsing emitted SystemVerilog.

use crate::ast::{BinOp, Expr, ExprKind, FifoKind, LitKind, UnaryOp};
use crate::lexer::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormalSignalKind {
    Input,
    Output,
    Reg,
    Wire,
}

#[derive(Debug, Clone)]
pub struct FormalSignal {
    pub name: String,
    pub width: u32,
    pub signed: bool,
    pub kind: FormalSignalKind,
}

#[derive(Debug, Clone)]
pub struct FormalCombEquation {
    pub target: String,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct FormalNextEquation {
    pub target: String,
    pub cond: Expr,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct FormalDerivedNonZero {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, Default)]
pub struct ConstructFormalModel {
    pub signals: Vec<FormalSignal>,
    pub resets: Vec<(String, Expr)>,
    pub comb_equations: Vec<FormalCombEquation>,
    pub next_equations: Vec<FormalNextEquation>,
    pub derived_nonzero: Vec<FormalDerivedNonZero>,
}

#[derive(Debug, Clone, Copy)]
pub enum CreditChannelRole {
    Sender,
    Receiver,
}

#[derive(Debug, Clone)]
pub struct CreditChannelFormalSpec {
    pub port_name: String,
    pub channel_name: String,
    pub role: CreditChannelRole,
    pub depth: u64,
    pub payload_width: Option<u32>,
    pub merged: bool,
    pub span: Span,
}

impl ConstructFormalModel {
    pub fn credit_channel(spec: &CreditChannelFormalSpec) -> Self {
        let mut model = ConstructFormalModel::default();
        let port = &spec.port_name;
        let ch = &spec.channel_name;
        let span = spec.span;
        let cnt_width = clog2_u32(spec.depth + 1).max(1);
        let ptr_width = clog2_u32(spec.depth);

        let send_valid_name = format!("{port}_{ch}_send_valid");
        let send_data_name = format!("{port}_{ch}_send_data");
        let credit_return_name = format!("{port}_{ch}_credit_return");
        let handshake_kind = |unmerged: FormalSignalKind| {
            if spec.merged {
                FormalSignalKind::Wire
            } else {
                unmerged
            }
        };
        let add_signal =
            |model: &mut ConstructFormalModel, name: String, width: u32, kind: FormalSignalKind| {
                model.signals.push(FormalSignal {
                    name,
                    width,
                    signed: false,
                    kind,
                });
            };

        match spec.role {
            CreditChannelRole::Sender => {
                let credit_name = format!("__{port}_{ch}_credit");
                add_signal(
                    &mut model,
                    credit_name.clone(),
                    cnt_width,
                    FormalSignalKind::Reg,
                );
                model
                    .resets
                    .push((credit_name.clone(), sized_lit(cnt_width, spec.depth, span)));
                add_signal(
                    &mut model,
                    send_valid_name.clone(),
                    1,
                    handshake_kind(FormalSignalKind::Output),
                );
                add_signal(
                    &mut model,
                    credit_return_name.clone(),
                    1,
                    handshake_kind(FormalSignalKind::Input),
                );
                if let Some(width) = spec.payload_width {
                    add_signal(
                        &mut model,
                        send_data_name,
                        width,
                        handshake_kind(FormalSignalKind::Output),
                    );
                }

                let send_valid = ident(&send_valid_name, span);
                let credit_return = ident(&credit_return_name, span);
                let dec_cond = and(send_valid.clone(), not(credit_return.clone(), span), span);
                let inc_cond = and(not(send_valid, span), credit_return, span);
                let credit = ident(&credit_name, span);
                let one = sized_lit(1, 1, span);
                model.next_equations.push(FormalNextEquation {
                    target: credit_name.clone(),
                    cond: dec_cond,
                    value: bin(BinOp::Sub, credit.clone(), one.clone(), span),
                });
                model.next_equations.push(FormalNextEquation {
                    target: credit_name.clone(),
                    cond: inc_cond,
                    value: bin(BinOp::Add, credit, one, span),
                });
                model.derived_nonzero.push(FormalDerivedNonZero {
                    name: format!("__{port}_{ch}_can_send"),
                    source: credit_name,
                });
            }
            CreditChannelRole::Receiver => {
                let occ_name = format!("__{port}_{ch}_occ");
                add_signal(
                    &mut model,
                    occ_name.clone(),
                    cnt_width,
                    FormalSignalKind::Reg,
                );
                model
                    .resets
                    .push((occ_name.clone(), sized_lit(cnt_width, 0, span)));
                if ptr_width > 0 {
                    for name in [format!("__{port}_{ch}_head"), format!("__{port}_{ch}_tail")] {
                        add_signal(&mut model, name.clone(), ptr_width, FormalSignalKind::Reg);
                        model.resets.push((name, sized_lit(ptr_width, 0, span)));
                    }
                }
                add_signal(
                    &mut model,
                    send_valid_name.clone(),
                    1,
                    handshake_kind(FormalSignalKind::Input),
                );
                add_signal(
                    &mut model,
                    credit_return_name.clone(),
                    1,
                    handshake_kind(FormalSignalKind::Output),
                );
                if let Some(width) = spec.payload_width {
                    add_signal(
                        &mut model,
                        send_data_name,
                        width,
                        handshake_kind(FormalSignalKind::Input),
                    );
                }

                let send_valid = ident(&send_valid_name, span);
                let credit_return = ident(&credit_return_name, span);
                let push_only = and(send_valid.clone(), not(credit_return.clone(), span), span);
                let pop_only = and(not(send_valid.clone(), span), credit_return.clone(), span);
                let occ = ident(&occ_name, span);
                let one = sized_lit(1, 1, span);
                model.next_equations.push(FormalNextEquation {
                    target: occ_name.clone(),
                    cond: push_only,
                    value: bin(BinOp::Add, occ.clone(), one.clone(), span),
                });
                model.next_equations.push(FormalNextEquation {
                    target: occ_name.clone(),
                    cond: pop_only,
                    value: bin(BinOp::Sub, occ, one, span),
                });

                if ptr_width > 0 {
                    let depth_lit = sized_lit(ptr_width + 1, spec.depth, span);
                    let one_ptr = sized_lit(1, 1, span);
                    let head_name = format!("__{port}_{ch}_head");
                    let tail_name = format!("__{port}_{ch}_tail");
                    let head_plus = bin(BinOp::Add, ident(&head_name, span), one_ptr.clone(), span);
                    model.next_equations.push(FormalNextEquation {
                        target: head_name,
                        cond: credit_return,
                        value: bin(BinOp::Mod, head_plus, depth_lit.clone(), span),
                    });
                    let tail_plus = bin(BinOp::Add, ident(&tail_name, span), one_ptr, span);
                    model.next_equations.push(FormalNextEquation {
                        target: tail_name,
                        cond: send_valid,
                        value: bin(BinOp::Mod, tail_plus, depth_lit, span),
                    });
                }
                model.derived_nonzero.push(FormalDerivedNonZero {
                    name: format!("__{port}_{ch}_valid"),
                    source: occ_name,
                });
            }
        }

        model
    }
}

#[derive(Debug, Clone)]
pub struct FifoFormalModel {
    pub name: String,
    pub kind: FifoKind,
    pub depth: u64,
    pub data_width: u64,
    pub overflow: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum ArbiterFormalPolicy {
    Priority,
    RoundRobin,
}

#[derive(Debug, Clone)]
pub struct ArbiterFormalModel {
    pub name: String,
    pub policy: ArbiterFormalPolicy,
    pub num_req: u64,
    pub latency: u32,
}

pub fn render_lean_fifo_equations(out: &mut String, base: &str, model: &FifoFormalModel) {
    match model.kind {
        FifoKind::Fifo => render_lean_sync_fifo_equations(out, base),
        FifoKind::Lifo => render_lean_lifo_equations(out, base),
    }
}

pub fn render_lean_arbiter_equations(out: &mut String, base: &str, model: &ArbiterFormalModel) {
    match model.policy {
        ArbiterFormalPolicy::Priority => render_lean_priority_arbiter_equations(out, base),
        ArbiterFormalPolicy::RoundRobin => render_lean_round_robin_arbiter_equations(out, base),
    }
}

pub fn render_smt2_fifo_sanity(model: &FifoFormalModel) -> String {
    render_smt2_fifo_sanity_with_prefix(model, &smt_ident(&model.name))
}

pub fn render_smt2_fifo_sanity_with_prefix(model: &FifoFormalModel, prefix: &str) -> String {
    let width = clog2_u32(model.depth + 1).max(1);
    let depth = bv_lit(model.depth, width);
    let zero = bv_lit(0, width);
    let one = bv_lit(1, width);
    let occ = format!("{prefix}_occ");
    let push_valid = format!("{prefix}_push_valid");
    let pop_ready = format!("{prefix}_pop_ready");
    let full = format!("{prefix}_full");
    let empty = format!("{prefix}_empty");
    let push_ready = format!("{prefix}_push_ready");
    let pop_valid = format!("{prefix}_pop_valid");
    let do_push = format!("{prefix}_do_push");
    let do_pop = format!("{prefix}_do_pop");
    let next_occ = format!("{prefix}_next_occ");
    let mut out = String::new();
    out.push_str("; auto-generated construct formal IR sanity check\n");
    out.push_str("; model: FIFO/LIFO control equations\n");
    out.push_str("(set-logic QF_BV)\n");
    out.push_str(&format!("(declare-fun {occ} () (_ BitVec {width}))\n"));
    out.push_str(&format!("(declare-fun {push_valid} () Bool)\n"));
    out.push_str(&format!("(declare-fun {pop_ready} () Bool)\n"));
    out.push_str(&format!("(assert (bvule {occ} {depth}))\n"));
    out.push_str(&format!("(define-fun {full} () Bool (= {occ} {depth}))\n"));
    out.push_str(&format!("(define-fun {empty} () Bool (= {occ} {zero}))\n"));
    out.push_str(&format!("(define-fun {push_ready} () Bool (not {full}))\n"));
    out.push_str(&format!("(define-fun {pop_valid} () Bool (not {empty}))\n"));
    out.push_str(&format!(
        "(define-fun {do_push} () Bool (and {push_valid} {push_ready}))\n"
    ));
    out.push_str(&format!(
        "(define-fun {do_pop} () Bool (and {pop_ready} {pop_valid}))\n"
    ));
    out.push_str(&format!(
        "(define-fun {next_occ} () (_ BitVec {width}) (ite (and {do_push} (not {do_pop})) (bvadd {occ} {one}) (ite (and (not {do_push}) {do_pop}) (bvsub {occ} {one}) {occ})))\n"
    ));
    out.push_str("(assert (not (and\n");
    out.push_str(&format!("  (= {push_ready} (not {full}))\n"));
    out.push_str(&format!("  (= {pop_valid} (not {empty}))\n"));
    out.push_str(&format!("  (bvule {next_occ} {depth})\n"));
    out.push_str(")))\n");
    out.push_str("(check-sat)\n");
    out
}

pub fn render_smt2_arbiter_sanity(model: &ArbiterFormalModel) -> String {
    render_smt2_arbiter_sanity_with_prefix(model, &smt_ident(&model.name))
}

pub fn render_smt2_arbiter_sanity_with_prefix(model: &ArbiterFormalModel, prefix: &str) -> String {
    let n = model.num_req.max(1) as usize;
    let req = format!("{prefix}_req");
    let grant = format!("{prefix}_grant");
    let start_name = format!("{prefix}_start");
    let start_width = clog2_u32(model.num_req + 1).max(1);
    let grant_expr = match model.policy {
        ArbiterFormalPolicy::Priority => priority_grant_expr(&req, n, &(0..n).collect::<Vec<_>>()),
        ArbiterFormalPolicy::RoundRobin => {
            let mut expr = bv_zero(n);
            for start in (0..n).rev() {
                let order = (0..n).map(|off| (start + off) % n).collect::<Vec<_>>();
                expr = format!(
                    "(ite (= {start_name} {}) {} {expr})",
                    bv_lit(start as u64, start_width),
                    priority_grant_expr(&req, n, &order),
                );
            }
            expr
        }
    };
    let zero = bv_zero(n);
    let one = onehot_lit(n, 0);
    let mut out = String::new();
    out.push_str("; auto-generated construct formal IR sanity check\n");
    out.push_str("; model: arbiter grant equations\n");
    out.push_str("(set-logic QF_BV)\n");
    out.push_str(&format!("(declare-fun {req} () (_ BitVec {n}))\n"));
    if matches!(model.policy, ArbiterFormalPolicy::RoundRobin) {
        out.push_str(&format!(
            "(declare-fun {start_name} () (_ BitVec {start_width}))\n"
        ));
        out.push_str(&format!(
            "(assert (bvult {start_name} {}))\n",
            bv_lit(model.num_req, start_width)
        ));
    }
    out.push_str(&format!(
        "(define-fun {grant} () (_ BitVec {n}) {grant_expr})\n"
    ));
    out.push_str("(assert (not (and\n");
    out.push_str(&format!("  (= (bvand {grant} (bvnot {req})) {zero})\n"));
    out.push_str(&format!(
        "  (or (= {grant} {zero}) (= (bvand {grant} (bvsub {grant} {one})) {zero}))\n"
    ));
    out.push_str(")))\n");
    out.push_str("(check-sat)\n");
    out
}

fn render_lean_sync_fifo_equations(out: &mut String, base: &str) {
    out.push_str(&format!(
        "\ndef {base}_sync_equations : Fifo.SyncGenerated {base} (BitVec {base}.dataWidth) :=\n"
    ));
    out.push_str("  { full := fun wrPtr rdPtr => (Fifo.ptrOccupancy ");
    out.push_str(base);
    out.push_str(" wrPtr rdPtr == ");
    out.push_str(base);
    out.push_str(".depth)\n");
    out.push_str("    empty := fun wrPtr rdPtr => (Fifo.ptrOccupancy ");
    out.push_str(base);
    out.push_str(" wrPtr rdPtr == 0)\n");
    out.push_str("    pushReady := fun wrPtr rdPtr => !((Fifo.ptrOccupancy ");
    out.push_str(base);
    out.push_str(" wrPtr rdPtr == ");
    out.push_str(base);
    out.push_str(".depth))\n");
    out.push_str("    popValid := fun wrPtr rdPtr => !((Fifo.ptrOccupancy ");
    out.push_str(base);
    out.push_str(" wrPtr rdPtr == 0))\n");
    out.push_str("    writeIndex := fun wrPtr => Fifo.ptrIndex ");
    out.push_str(base);
    out.push_str(" wrPtr\n");
    out.push_str("    readIndex := fun rdPtr => Fifo.ptrIndex ");
    out.push_str(base);
    out.push_str(" rdPtr\n");
    out.push_str("    nextWrPtr := fun wrPtr doPush => if doPush then (wrPtr + 1) % Fifo.ptrMod ");
    out.push_str(base);
    out.push_str(" else wrPtr\n");
    out.push_str("    nextRdPtr := fun rdPtr doPop => if doPop then (rdPtr + 1) % Fifo.ptrMod ");
    out.push_str(base);
    out.push_str(" else rdPtr\n");
    out.push_str("    nextMem := fun mem wrPtr data doPush => if doPush then Fifo.updateMem mem (Fifo.ptrIndex ");
    out.push_str(base);
    out.push_str(" wrPtr) data else mem\n");
    out.push_str("    full_eq := by intro wrPtr rdPtr; rfl\n");
    out.push_str("    empty_eq := by intro wrPtr rdPtr; rfl\n");
    out.push_str("    push_ready_eq := by intro wrPtr rdPtr; rfl\n");
    out.push_str("    pop_valid_eq := by intro wrPtr rdPtr; rfl\n");
    out.push_str("    write_index_eq := by intro wrPtr; rfl\n");
    out.push_str("    read_index_eq := by intro rdPtr; rfl\n");
    out.push_str("    next_wr_ptr_eq := by intro wrPtr doPush; rfl\n");
    out.push_str("    next_rd_ptr_eq := by intro rdPtr doPop; rfl\n");
    out.push_str("    next_mem_eq := by intro mem wrPtr data doPush; rfl }\n\n");
}

fn render_lean_lifo_equations(out: &mut String, base: &str) {
    out.push_str(&format!(
        "\ndef {base}_lifo_equations : Fifo.LifoGenerated {base} (BitVec {base}.dataWidth) :=\n"
    ));
    out.push_str("  { full := fun sp => (sp == ");
    out.push_str(base);
    out.push_str(".depth)\n");
    out.push_str("    empty := fun sp => (sp == 0)\n");
    out.push_str("    pushReady := fun sp => !((sp == ");
    out.push_str(base);
    out.push_str(".depth))\n");
    out.push_str("    popValid := fun sp => !((sp == 0))\n");
    out.push_str("    writeIndex := fun sp doPop => if doPop then sp - 1 else sp\n");
    out.push_str("    readIndex := fun sp => sp - 1\n");
    out.push_str("    nextSp := fun sp doPush doPop => if doPush && doPop then sp else if doPush then sp + 1 else if doPop then sp - 1 else sp\n");
    out.push_str("    nextMem := fun mem sp data doPush doPop => if doPush then Fifo.updateMem mem (if doPop then sp - 1 else sp) data else mem\n");
    out.push_str("    full_eq := by intro sp; rfl\n");
    out.push_str("    empty_eq := by intro sp; rfl\n");
    out.push_str("    push_ready_eq := by intro sp; rfl\n");
    out.push_str("    pop_valid_eq := by intro sp; rfl\n");
    out.push_str("    write_index_eq := by intro sp doPop; rfl\n");
    out.push_str("    read_index_eq := by intro sp; rfl\n");
    out.push_str("    next_sp_eq := by intro sp doPush doPop; rfl\n");
    out.push_str("    next_mem_eq := by intro mem sp data doPush doPop; rfl }\n\n");
}

fn render_lean_priority_arbiter_equations(out: &mut String, base: &str) {
    out.push_str(&format!(
        "\ndef {base}_priority_equations : Arbiter.PriorityGenerated {base} :=\n"
    ));
    out.push_str("  { readySelected := fun req idx => Arbiter.priorityReady req idx\n");
    out.push_str("    readyVector := fun req idx => Arbiter.oneHot ");
    out.push_str(base);
    out.push_str(" idx\n");
    out.push_str("    ready_selected_eq := by intro req idx; rfl\n");
    out.push_str("    ready_vector_eq := by intro req idx h; rfl }\n\n");
}

fn render_lean_round_robin_arbiter_equations(out: &mut String, base: &str) {
    out.push_str(&format!(
        "\ndef {base}_round_robin_equations : Arbiter.RoundRobinGenerated {base} :=\n"
    ));
    out.push_str(
        "  { readySelected := fun start req idx => Arbiter.roundRobinReady start req idx\n",
    );
    out.push_str("    readyVector := fun start req idx => Arbiter.oneHot ");
    out.push_str(base);
    out.push_str(" idx\n");
    out.push_str("    nextPtr := fun start idx => if idx.val + 1 = ");
    out.push_str(base);
    out.push_str(".numReq then 0 else (idx.val + 1) % ");
    out.push_str(base);
    out.push_str(".numReq\n");
    out.push_str("    ready_selected_eq := by intro start req idx; rfl\n");
    out.push_str("    ready_vector_eq := by intro start req idx h; rfl\n");
    out.push_str("    next_ptr_eq := by intro start idx; rfl }\n\n");
}

fn clog2_u32(value: u64) -> u32 {
    if value <= 1 {
        0
    } else {
        (value - 1).ilog2() + 1
    }
}

fn priority_grant_expr(req: &str, width: usize, order: &[usize]) -> String {
    let mut expr = bv_zero(width);
    for idx in order.iter().rev() {
        expr = format!(
            "(ite (= ((_ extract {idx} {idx}) {req}) #b1) {} {expr})",
            onehot_lit(width, *idx),
        );
    }
    expr
}

fn bv_zero(width: usize) -> String {
    format!("#b{}", "0".repeat(width.max(1)))
}

fn bv_lit(value: u64, width: u32) -> String {
    let width = width.max(1) as usize;
    let mask = if width >= 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    };
    format!("#b{:0width$b}", value & mask, width = width)
}

fn onehot_lit(width: usize, idx: usize) -> String {
    let mut bits = vec![b'0'; width.max(1)];
    let pos = width.saturating_sub(1).saturating_sub(idx);
    bits[pos] = b'1';
    String::from_utf8(bits).map(|s| format!("#b{s}")).unwrap()
}

fn smt_ident(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() || out.as_bytes()[0].is_ascii_digit() {
        out.insert_str(0, "construct_");
    }
    out
}

fn sized_lit(width: u32, value: u64, span: Span) -> Expr {
    Expr::new(ExprKind::Literal(LitKind::Sized(width, value)), span)
}

fn ident(name: &str, span: Span) -> Expr {
    Expr::new(ExprKind::Ident(name.to_string()), span)
}

fn bin(op: BinOp, a: Expr, b: Expr, span: Span) -> Expr {
    Expr::new(ExprKind::Binary(op, Box::new(a), Box::new(b)), span)
}

fn not(a: Expr, span: Span) -> Expr {
    Expr::new(ExprKind::Unary(UnaryOp::Not, Box::new(a)), span)
}

fn and(a: Expr, b: Expr, span: Span) -> Expr {
    bin(BinOp::And, a, b, span)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::{Command, Stdio};

    #[test]
    fn credit_channel_receiver_model_has_queue_equations() {
        let model = ConstructFormalModel::credit_channel(&CreditChannelFormalSpec {
            port_name: "chwire".to_string(),
            channel_name: "data".to_string(),
            role: CreditChannelRole::Receiver,
            depth: 4,
            payload_width: Some(8),
            merged: true,
            span: Span { start: 0, end: 0 },
        });

        assert!(model
            .signals
            .iter()
            .any(|s| s.name == "__chwire_data_occ" && s.kind == FormalSignalKind::Reg));
        assert!(model
            .signals
            .iter()
            .any(|s| s.name == "chwire_data_send_valid" && s.kind == FormalSignalKind::Wire));
        assert!(model
            .next_equations
            .iter()
            .any(|eq| eq.target == "__chwire_data_head"));
        assert!(model
            .derived_nonzero
            .iter()
            .any(|d| d.name == "__chwire_data_valid" && d.source == "__chwire_data_occ"));
    }

    #[test]
    fn fifo_smt2_sanity_is_unsat_under_z3() {
        let model = FifoFormalModel {
            name: "TxQueue".to_string(),
            kind: FifoKind::Fifo,
            depth: 4,
            data_width: 8,
            overflow: false,
        };
        let smt = render_smt2_fifo_sanity(&model);
        assert!(smt.contains("(set-logic QF_BV)"));
        assert!(smt.contains("next_occ"));
        assert_z3_unsat_or_skip(&smt);
    }

    #[test]
    fn round_robin_arbiter_smt2_sanity_is_unsat_under_z3() {
        let model = ArbiterFormalModel {
            name: "RR".to_string(),
            policy: ArbiterFormalPolicy::RoundRobin,
            num_req: 4,
            latency: 1,
        };
        let smt = render_smt2_arbiter_sanity(&model);
        assert!(smt.contains("(declare-fun RR_start () (_ BitVec 3))"));
        assert!(smt.contains("(assert (bvult RR_start #b100))"));
        assert!(smt.contains("(define-fun RR_grant"));
        assert_z3_unsat_or_skip(&smt);
    }

    fn assert_z3_unsat_or_skip(smt: &str) {
        let Ok(mut child) = Command::new("z3")
            .arg("-in")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
        else {
            eprintln!("skipping: z3 not in PATH");
            return;
        };
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(smt.as_bytes())
            .unwrap();
        let out = child.wait_with_output().expect("wait for z3");
        if !out.status.success() {
            panic!(
                "z3 failed\nstdout:\n{}\nstderr:\n{}\nSMT:\n{smt}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            stdout.trim(),
            "unsat",
            "expected unsat SMT sanity check:\n{smt}"
        );
    }
}
