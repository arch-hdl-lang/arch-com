//! Lean proof certificates for first-class ARCH constructs.
//!
//! This backend is intentionally per-instance: it reads the checked AST and
//! emits a small Lean replay file that instantiates reusable construct
//! theorems with the concrete FIFO/arbiter parameters seen by the compiler.

use crate::ast::{
    ArbiterDecl, ArbiterPolicy, Expr, ExprKind, FifoDecl, FifoKind, Item, LitKind, ParamKind,
    TypeExpr,
};
use crate::construct_formal_ir::{
    render_lean_arbiter_equations, render_lean_fifo_equations,
    render_smt2_arbiter_sanity_with_prefix, render_smt2_fifo_sanity_with_prefix,
    ArbiterFormalModel, ArbiterFormalPolicy, FifoFormalModel,
};

#[derive(Debug, Clone)]
struct FifoCert {
    model: FifoFormalModel,
}

#[derive(Debug, Clone)]
struct ArbiterCert {
    model: ArbiterFormalModel,
}

#[derive(Debug, Default)]
struct ConstructCerts {
    fifos: Vec<FifoCert>,
    arbiters: Vec<ArbiterCert>,
    unsupported: Vec<String>,
}

pub fn render_lean_checked_items<'a, I>(items: I) -> Result<String, String>
where
    I: IntoIterator<Item = &'a Item>,
{
    let certs = collect(items);
    if !certs.unsupported.is_empty() {
        return Err(certs.unsupported.join("\n"));
    }
    if certs.fifos.is_empty() && certs.arbiters.is_empty() {
        return Err("no supported fifo or arbiter constructs found".to_string());
    }
    Ok(render_lean(&certs))
}

pub fn render_lean_checked(source: &crate::ast::SourceFile) -> Result<String, String> {
    render_lean_checked_items(source.items.iter())
}

pub fn render_smt2_checked_items<'a, I>(items: I) -> Result<String, String>
where
    I: IntoIterator<Item = &'a Item>,
{
    let certs = collect(items);
    if !certs.unsupported.is_empty() {
        return Err(certs.unsupported.join("\n"));
    }
    if certs.fifos.is_empty() && certs.arbiters.is_empty() {
        return Err("no supported fifo or arbiter constructs found".to_string());
    }
    Ok(render_smt2(&certs))
}

pub fn render_smt2_checked(source: &crate::ast::SourceFile) -> Result<String, String> {
    render_smt2_checked_items(source.items.iter())
}

fn collect<'a, I>(items: I) -> ConstructCerts
where
    I: IntoIterator<Item = &'a Item>,
{
    let mut out = ConstructCerts::default();
    for item in items {
        match item {
            Item::Fifo(fifo) if !fifo.is_interface => match fifo_cert(fifo) {
                Ok(cert) => out.fifos.push(cert),
                Err(err) => out.unsupported.push(err),
            },
            Item::Arbiter(arbiter) if !arbiter.is_interface => match arbiter_cert(arbiter) {
                Ok(cert) => out.arbiters.push(cert),
                Err(err) => out.unsupported.push(err),
            },
            _ => {}
        }
    }
    out
}

fn fifo_cert(fifo: &FifoDecl) -> Result<FifoCert, String> {
    if crate::resolve::detect_async_fifo(&fifo.ports) {
        return Err(format!(
            "fifo `{}`: async FIFO proof is not supported yet",
            fifo.name.name
        ));
    }
    let depth = const_param_u64(fifo.params.as_slice(), "DEPTH")
        .transpose()?
        .unwrap_or(16);
    if depth == 0 {
        return Err(format!(
            "fifo `{}`: DEPTH must be greater than zero for Lean proof",
            fifo.name.name
        ));
    }

    let overflow = const_param_u64(fifo.params.as_slice(), "OVERFLOW")
        .transpose()?
        .unwrap_or(0);
    if overflow != 0 {
        return Err(format!(
            "fifo `{}`: OVERFLOW mode proof is not supported yet",
            fifo.name.name
        ));
    }

    let data_width = fifo
        .params
        .iter()
        .find_map(|p| match &p.kind {
            ParamKind::Type(ty) => Some(type_width_u64(ty)),
            _ => None,
        })
        .transpose()?
        .ok_or_else(|| {
            format!(
                "fifo `{}`: missing type parameter width for Lean proof",
                fifo.name.name
            )
        })?;
    if data_width == 0 {
        return Err(format!(
            "fifo `{}`: data width must be greater than zero for Lean proof",
            fifo.name.name
        ));
    }

    Ok(FifoCert {
        model: FifoFormalModel {
            name: fifo.name.name.clone(),
            kind: fifo.kind,
            depth,
            data_width,
            overflow: false,
        },
    })
}

fn arbiter_cert(arbiter: &ArbiterDecl) -> Result<ArbiterCert, String> {
    let num_req = const_param_u64(arbiter.params.as_slice(), "NUM_REQ")
        .transpose()?
        .unwrap_or(4);
    if num_req == 0 {
        return Err(format!(
            "arbiter `{}`: NUM_REQ must be greater than zero for Lean proof",
            arbiter.name.name
        ));
    }
    if arbiter.latency == 0 {
        return Err(format!(
            "arbiter `{}`: latency must be greater than zero for Lean proof",
            arbiter.name.name
        ));
    }
    let policy = match &arbiter.policy {
        ArbiterPolicy::Priority => ArbiterFormalPolicy::Priority,
        ArbiterPolicy::RoundRobin => ArbiterFormalPolicy::RoundRobin,
        ArbiterPolicy::Lru => {
            return Err(format!(
                "arbiter `{}`: lru policy proof is not supported yet",
                arbiter.name.name
            ));
        }
        ArbiterPolicy::Weighted(_) => {
            return Err(format!(
                "arbiter `{}`: weighted policy proof is not supported yet",
                arbiter.name.name
            ));
        }
        ArbiterPolicy::Custom(fn_name) => {
            return Err(format!(
                "arbiter `{}`: custom policy `{}` proof is not supported yet",
                arbiter.name.name, fn_name.name
            ));
        }
    };

    Ok(ArbiterCert {
        model: ArbiterFormalModel {
            name: arbiter.name.name.clone(),
            policy,
            num_req,
            latency: arbiter.latency,
        },
    })
}

fn const_param_u64(params: &[crate::ast::ParamDecl], name: &str) -> Option<Result<u64, String>> {
    params
        .iter()
        .find(|p| p.name.name == name)
        .and_then(|p| p.default.as_ref())
        .map(const_expr_u64)
}

fn const_expr_u64(expr: &Expr) -> Result<u64, String> {
    match &expr.kind {
        ExprKind::Literal(LitKind::Dec(v))
        | ExprKind::Literal(LitKind::Hex(v))
        | ExprKind::Literal(LitKind::Bin(v)) => Ok(*v),
        ExprKind::Literal(LitKind::Sized(_, v)) => Ok(*v),
        ExprKind::Clog2(inner) => {
            let v = const_expr_u64(inner)?;
            Ok(clog2_u64(v))
        }
        _ => Err(format!(
            "unsupported non-literal construct proof parameter expression at byte {}",
            expr.span.start
        )),
    }
}

fn type_width_u64(ty: &TypeExpr) -> Result<u64, String> {
    match ty {
        TypeExpr::UInt(width) | TypeExpr::SInt(width) => const_expr_u64(width),
        TypeExpr::Bool | TypeExpr::Bit => Ok(1),
        TypeExpr::Vec(elem, len) => {
            let elem_width = type_width_u64(elem)?;
            let len = const_expr_u64(len)?;
            elem_width.checked_mul(len).ok_or_else(|| {
                "construct proof type width overflow while multiplying Vec width".to_string()
            })
        }
        TypeExpr::Named(name) => Err(format!(
            "named type `{}` is not supported in construct proof data-width extraction",
            name.name
        )),
        TypeExpr::Clock(_) | TypeExpr::Reset(_, _) => {
            Err("clock/reset type is not a data payload type for construct proof".to_string())
        }
    }
}

fn clog2_u64(value: u64) -> u64 {
    if value <= 1 {
        0
    } else {
        u64::BITS as u64 - (value - 1).leading_zeros() as u64
    }
}

fn render_lean(certs: &ConstructCerts) -> String {
    let mut out = String::new();
    out.push_str("import ArchConstructProof\n\n");
    out.push_str("namespace Arch.ConstructProof.Generated\n\n");
    for fifo in &certs.fifos {
        push_fifo_lean(&mut out, fifo);
        out.push('\n');
    }
    for arbiter in &certs.arbiters {
        push_arbiter_lean(&mut out, arbiter);
        out.push('\n');
    }
    out.push_str("end Arch.ConstructProof.Generated\n");
    out
}

fn render_smt2(certs: &ConstructCerts) -> String {
    let mut out = String::new();
    out.push_str("; auto-generated by `arch build --emit-construct-proof-smt`\n");
    out.push_str("; each check-sat query is expected to be unsat\n\n");
    out.push_str("(set-logic QF_BV)\n\n");
    for (idx, fifo) in certs.fifos.iter().enumerate() {
        let prefix = smt_ident(&format!("{}_fifo_{idx}", fifo.model.name));
        out.push_str(&format!("; fifo {}\n", fifo.model.name));
        out.push_str("(push)\n");
        out.push_str(&strip_smt_logic(&render_smt2_fifo_sanity_with_prefix(
            &fifo.model,
            &prefix,
        )));
        out.push_str("(pop)\n\n");
    }
    for (idx, arbiter) in certs.arbiters.iter().enumerate() {
        let prefix = smt_ident(&format!("{}_arbiter_{idx}", arbiter.model.name));
        out.push_str(&format!("; arbiter {}\n", arbiter.model.name));
        out.push_str("(push)\n");
        out.push_str(&strip_smt_logic(&render_smt2_arbiter_sanity_with_prefix(
            &arbiter.model,
            &prefix,
        )));
        out.push_str("(pop)\n\n");
    }
    out
}

fn strip_smt_logic(smt: &str) -> String {
    smt.replace("(set-logic QF_BV)\n", "")
}

fn push_fifo_lean(out: &mut String, fifo: &FifoCert) {
    let model = &fifo.model;
    let base = lean_ident(&format!("{}_fifo", model.name));
    let kind = match model.kind {
        FifoKind::Fifo => "Fifo.Kind.fifo",
        FifoKind::Lifo => "Fifo.Kind.lifo",
    };
    out.push_str(&format!("def {base} : Fifo.Instance :=\n"));
    out.push_str(&format!(
        "  {{ name := {:?}, kind := {kind}, depth := {}, dataWidth := {}, overflow := {} }}\n\n",
        model.name,
        model.depth,
        model.data_width,
        if model.overflow { "true" } else { "false" }
    ));
    render_lean_fifo_equations(out, &base, model);
    out.push_str(&format!("theorem {base}_certificate :\n"));
    match model.kind {
        FifoKind::Fifo => {
            out.push_str(&format!(
                "    0 < {base}.depth /\\ 0 < {base}.dataWidth /\\ Fifo.SyncEquationsHold {base} {base}_sync_equations /\\ forall (contents : List (BitVec {base}.dataWidth)) (push : Option (BitVec {base}.dataWidth)) popReady,\n"
            ));
            out.push_str(&format!(
                "      Fifo.bounded {base} contents -> Fifo.bounded {base} (Fifo.step {base} contents push popReady) := by\n"
            ));
            out.push_str(&format!(
                "  exact Fifo.sync_certificate_checks {base} {base}_sync_equations (by native_decide) (by native_decide)\n"
            ));
        }
        FifoKind::Lifo => {
            out.push_str(&format!(
                "    0 < {base}.depth /\\ 0 < {base}.dataWidth /\\ Fifo.LifoEquationsHold {base} {base}_lifo_equations /\\ forall (contents : List (BitVec {base}.dataWidth)) (push : Option (BitVec {base}.dataWidth)) popReady,\n"
            ));
            out.push_str(&format!(
                "      Fifo.bounded {base} contents -> Fifo.bounded {base} (Fifo.step {base} contents push popReady) := by\n"
            ));
            out.push_str(&format!(
                "  exact Fifo.lifo_certificate_checks {base} {base}_lifo_equations (by native_decide) (by native_decide)\n"
            ));
        }
    }
}

fn push_arbiter_lean(out: &mut String, arbiter: &ArbiterCert) {
    let model = &arbiter.model;
    let base = lean_ident(&format!("{}_arbiter", model.name));
    let policy = match model.policy {
        ArbiterFormalPolicy::Priority => "Arbiter.Policy.priority",
        ArbiterFormalPolicy::RoundRobin => "Arbiter.Policy.roundRobin",
    };
    out.push_str(&format!("def {base} : Arbiter.Instance :=\n"));
    out.push_str(&format!(
        "  {{ name := {:?}, numReq := {}, policy := {policy}, latency := {} }}\n\n",
        model.name, model.num_req, model.latency
    ));
    render_lean_arbiter_equations(out, &base, model);
    out.push_str(&format!("theorem {base}_certificate :\n"));
    match model.policy {
        ArbiterFormalPolicy::Priority => {
            out.push_str(&format!(
                "    0 < {base}.numReq /\\ 0 < {base}.latency /\\ Arbiter.PriorityEquationsHold {base} {base}_priority_equations /\\ Arbiter.CorrectGrant {base} := by\n"
            ));
            out.push_str(&format!(
                "  exact Arbiter.priority_certificate_checks {base} {base}_priority_equations (by native_decide) (by native_decide)\n"
            ));
        }
        ArbiterFormalPolicy::RoundRobin => {
            out.push_str(&format!(
                "    0 < {base}.numReq /\\ 0 < {base}.latency /\\ Arbiter.RoundRobinEquationsHold {base} {base}_round_robin_equations /\\ Arbiter.CorrectGrant {base} := by\n"
            ));
            out.push_str(&format!(
                "  exact Arbiter.round_robin_certificate_checks {base} {base}_round_robin_equations (by native_decide) (by native_decide)\n"
            ));
        }
    }
}

fn lean_ident(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() || out.as_bytes()[0].is_ascii_digit() {
        out.insert_str(0, "cert_");
    }
    out
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
        out.insert_str(0, "cert_");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;
    use crate::parser::Parser;

    fn parse_source(src: &str) -> crate::ast::SourceFile {
        let tokens = tokenize(src).unwrap();
        let mut parser = Parser::new(tokens, src);
        parser.parse_source_file().unwrap()
    }

    #[test]
    fn emits_fifo_and_round_robin_certificates() {
        let source = parse_source(
            r#"
domain SysDomain
end domain SysDomain

fifo TxQueue
  param DEPTH: const = 16;
  param T: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port push_valid: in Bool;
  port push_ready: out Bool;
  port push_data: in T;
  port pop_valid: out Bool;
  port pop_ready: in Bool;
  port pop_data: out T;
end fifo TxQueue

arbiter BusArbiter
  policy round_robin;
  param NUM_REQ: const = 4;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  ports[NUM_REQ] request
    valid: in Bool;
    ready: out Bool;
  end ports request
  port grant_valid: out Bool;
  port grant_requester: out UInt<2>;
end arbiter BusArbiter
"#,
        );
        let lean = render_lean_checked(&source).unwrap();
        assert!(lean.contains("def TxQueue_fifo : Fifo.Instance"));
        assert!(lean.contains("Fifo.SyncEquationsHold TxQueue_fifo TxQueue_fifo_sync_equations"));
        assert!(lean.contains("def BusArbiter_arbiter : Arbiter.Instance"));
        assert!(lean.contains(
            "Arbiter.RoundRobinEquationsHold BusArbiter_arbiter BusArbiter_arbiter_round_robin_equations"
        ));
    }

    #[test]
    fn emits_fifo_and_arbiter_smt_certificates() {
        let source = parse_source(
            r#"
domain SysDomain
end domain SysDomain

fifo TxQueue
  param DEPTH: const = 16;
  param T: type = UInt<8>;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port push_valid: in Bool;
  port push_ready: out Bool;
  port push_data: in T;
  port pop_valid: out Bool;
  port pop_ready: in Bool;
  port pop_data: out T;
end fifo TxQueue

arbiter BusArbiter
  policy round_robin;
  param NUM_REQ: const = 3;
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  ports[NUM_REQ] request
    valid: in Bool;
    ready: out Bool;
  end ports request
  port grant_valid: out Bool;
  port grant_requester: out UInt<2>;
end arbiter BusArbiter
"#,
        );
        let smt = render_smt2_checked(&source).unwrap();
        assert!(smt.contains("; fifo TxQueue"));
        assert!(smt.contains("; arbiter BusArbiter"));
        assert_eq!(smt.matches("(check-sat)").count(), 2);
        assert!(smt.contains("BusArbiter_arbiter_0_start"));
    }

    #[test]
    fn rejects_async_fifo() {
        let source = parse_source(
            r#"
domain Wr
end domain Wr
domain Rd
end domain Rd

fifo AsyncQueue
  param DEPTH: const = 16;
  param T: type = UInt<8>;
  port wr_clk: in Clock<Wr>;
  port rd_clk: in Clock<Rd>;
  port rst: in Reset<Async>;
  port push_valid: in Bool;
  port push_ready: out Bool;
  port push_data: in T;
  port pop_valid: out Bool;
  port pop_ready: in Bool;
  port pop_data: out T;
end fifo AsyncQueue
"#,
        );
        let err = render_lean_checked(&source).unwrap_err();
        assert!(err.contains("async FIFO proof is not supported yet"));
    }
}
