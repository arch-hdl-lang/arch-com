//! `gen_pipeline` emitter + its helpers — extracted from
//! `sim_codegen/mod.rs`. Lives in a submodule of `sim_codegen` so
//! `super::` keeps visibility of the shared free-function helpers without
//! widening visibility modifiers on them.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use super::{SimCodegen, SimModel};
use super::*;

impl<'a> SimCodegen<'a> {
    pub(crate) fn gen_pipeline(&self, p: &PipelineDecl) -> SimModel {
        let name = &p.name.name;
        let class = format!("V{name}");
        let _enum_map = build_enum_map(self.symbols);

        let port_names: HashSet<String> = p.ports.iter()
            .map(|pt| pt.name.name.clone()).collect();
        let stage_names: Vec<String> = p.stages.iter()
            .map(|s| s.name.name.clone()).collect();
        let stage_prefixes: Vec<String> = stage_names.iter()
            .map(|s| s.to_lowercase()).collect();

        // Flatten stage registers and let bindings
        struct StageReg { prefixed: String, ty: String, reset_val: String, bits: u32, is_let: bool }
        let mut all_regs: Vec<StageReg> = Vec::new();
        let mut stage_reg_names: Vec<HashSet<String>> = Vec::new();

        for (si, stage) in p.stages.iter().enumerate() {
            let prefix = &stage_prefixes[si];
            let mut names_set = HashSet::new();
            for item in &stage.body {
                match item {
                    ModuleBodyItem::RegDecl(r) => {
                        let prefixed = format!("{}_{}", prefix, r.name.name);
                        let ty = cpp_internal_type(&r.ty);
                        let bits = type_bits_te(&r.ty);
                        let reset_val = Self::pipeline_reset_value(&r.reset).unwrap_or("0".to_string());
                        names_set.insert(r.name.name.clone());
                        all_regs.push(StageReg { prefixed, ty, reset_val, bits, is_let: false });
                    }
                    ModuleBodyItem::LetBinding(l) => {
                        // ty=None means assignment to existing port/wire — skip as new binding
                        if l.ty.is_none() { continue; }
                        let prefixed = format!("{}_{}", prefix, l.name.name);
                        let ty = if let Some(ref te) = l.ty { cpp_internal_type(te) } else { "uint32_t".to_string() };
                        let bits = if let Some(ref te) = l.ty { type_bits_te(te) } else { 32 };
                        names_set.insert(l.name.name.clone());
                        all_regs.push(StageReg { prefixed, ty, reset_val: String::new(), bits, is_let: true });
                    }
                    _ => {}
                }
            }
            stage_reg_names.push(names_set);
        }

        let mut reg_names: HashSet<String> = HashSet::new();
        let mut let_names: HashSet<String> = HashSet::new();
        let mut widths: HashMap<String, u32> = HashMap::new();
        for sr in &all_regs {
            if sr.is_let { let_names.insert(sr.prefixed.clone()); }
            else { reg_names.insert(sr.prefixed.clone()); }
            widths.insert(sr.prefixed.clone(), sr.bits);
        }
        for prefix in &stage_prefixes {
            reg_names.insert(format!("{}_valid_r", prefix));
            widths.insert(format!("{}_valid_r", prefix), 1);
        }
        // Add port widths
        for pt in &p.ports {
            widths.insert(pt.name.name.clone(), type_bits_te(&pt.ty));
        }

        // ── Collect implicit wires (comb-block LHS targets + inst output
        // connection targets that aren't ports/regs/lets). These need to
        // be declared as members so eval_comb-emitted writes and seq-block
        // reads both compile. Type is inferred by walking the stage body
        // to find a consumer register or matching sub-port type.
        struct ImplicitWire { name: String, prefixed: String, ty_cpp: String, bits: u32 }
        let mut implicit_wires: Vec<Vec<ImplicitWire>> = Vec::new();
        for (si, stage) in p.stages.iter().enumerate() {
            let prefix = &stage_prefixes[si];
            let mut wires: Vec<ImplicitWire> = Vec::new();
            // Helper: find a consumer reg's TypeExpr (e.g. `alu_result <= alu_out`).
            let consumer_ty = |target: &str| -> Option<TypeExpr> {
                for it in &stage.body {
                    if let ModuleBodyItem::RegBlock(rb) = it {
                        for stmt in &rb.stmts {
                            if let Stmt::Assign(a) = stmt {
                                if let ExprKind::Ident(rhs) = &a.value.kind {
                                    if rhs == target {
                                        if let ExprKind::Ident(lhs) = &a.target.kind {
                                            for it2 in &stage.body {
                                                if let ModuleBodyItem::RegDecl(r) = it2 {
                                                    if r.name.name == *lhs { return Some(r.ty.clone()); }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                None
            };
            // Pass 1: comb-block LHS targets.
            fn walk_comb_targets(stmts: &[Stmt], out: &mut Vec<String>) {
                for s in stmts {
                    match s {
                        Stmt::Assign(a) => {
                            if let ExprKind::Ident(n) = &a.target.kind { out.push(n.clone()); }
                        }
                        Stmt::IfElse(ie) => {
                            walk_comb_targets(&ie.then_stmts, out);
                            walk_comb_targets(&ie.else_stmts, out);
                        }
                        _ => {}
                    }
                }
            }
            let is_known = |n: &str, wires: &Vec<ImplicitWire>| -> bool {
                port_names.contains(n)
                    || stage_reg_names[si].contains(n)
                    || wires.iter().any(|w| w.name == n)
            };
            for it in &stage.body {
                if let ModuleBodyItem::CombBlock(cb) = it {
                    let mut targets = Vec::new();
                    walk_comb_targets(&cb.stmts, &mut targets);
                    for t in targets {
                        if is_known(&t, &wires) { continue; }
                        let ty_te = consumer_ty(&t).unwrap_or(TypeExpr::UInt(Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(32)), p.span))));
                        let bits = type_bits_te(&ty_te);
                        let ty_cpp = cpp_internal_type(&ty_te);
                        wires.push(ImplicitWire { name: t.clone(), prefixed: format!("{prefix}_{t}"), ty_cpp, bits });
                    }
                }
            }
            // Pass 2: inst-output connection targets.
            for it in &stage.body {
                if let ModuleBodyItem::Inst(inst) = it {
                    let sub_ports = self.lookup_inst_ports(&inst.module_name.name);
                    for conn in &inst.connections {
                        if conn.direction != ConnectDir::Output { continue; }
                        let ExprKind::Ident(target) = &conn.signal.kind else { continue; };
                        if is_known(target, &wires) { continue; }
                        // Type from sub-module's matching port, fall back to consumer reg.
                        let ty_te = sub_ports.iter()
                            .find(|p| p.name.name == conn.port_name.name)
                            .map(|p| p.ty.clone())
                            .or_else(|| consumer_ty(target))
                            .unwrap_or(TypeExpr::UInt(Box::new(Expr::new(ExprKind::Literal(LitKind::Dec(32)), p.span))));
                        let bits = type_bits_te(&ty_te);
                        let ty_cpp = cpp_internal_type(&ty_te);
                        wires.push(ImplicitWire { name: target.clone(), prefixed: format!("{prefix}_{target}"), ty_cpp, bits });
                    }
                }
            }
            implicit_wires.push(wires);
        }
        // Register implicit wires in name-resolution sets so reads/writes
        // resolve to `<prefix>_<name>` (matching the let-binding convention).
        for (si, wires) in implicit_wires.iter().enumerate() {
            for w in wires {
                stage_reg_names[si].insert(w.name.clone());
                let_names.insert(w.prefixed.clone());
                widths.insert(w.prefixed.clone(), w.bits);
            }
        }

        // Collect insts per stage for codegen.
        let stage_insts: Vec<Vec<&InstDecl>> = p.stages.iter()
            .map(|s| s.body.iter().filter_map(|it| if let ModuleBodyItem::Inst(i) = it { Some(i) } else { None }).collect())
            .collect();

        let (rst_name, _is_async, is_low) = extract_reset_info(&p.ports);
        let rst_cond = if is_low { format!("(!{})", rst_name) } else { rst_name.clone() };
        let clk_name = p.ports.iter()
            .find(|pt| matches!(pt.ty, TypeExpr::Clock(_)))
            .map(|pt| pt.name.name.clone())
            .unwrap_or("clk".to_string());

        // ── Header ──
        let mut h = String::new();
        h.push_str("#pragma once\n#include <cstdint>\n#include <cstdio>\n#include \"verilated.h\"\n");
        // Include sub-module headers for any insts inside stages.
        let mut included: HashSet<String> = HashSet::new();
        for stage_list in &stage_insts {
            for inst in stage_list {
                if included.insert(inst.module_name.name.clone()) {
                    h.push_str(&format!("#include \"V{}.h\"\n", inst.module_name.name));
                }
            }
        }
        h.push('\n');
        for param in &p.params {
            if matches!(param.kind, ParamKind::Const | ParamKind::WidthConst(..)) {
                if let Some(ref def) = param.default {
                    let val = eval_const_expr(def);
                    h.push_str(&format!("#ifndef {}\n#define {} {val}ULL\n#endif\n", param.name.name, param.name.name));
                }
            }
        }
        h.push_str(&format!("\nclass {class} {{\npublic:\n"));
        for pt in &p.ports {
            h.push_str(&format!("  {} {};\n", cpp_port_type(&pt.ty), pt.name.name));
        }
        h.push('\n');

        // Constructor
        let mut inits: Vec<String> = p.ports.iter().map(|pt| format!("{}(0)", pt.name.name)).collect();
        inits.push("_clk_prev(0)".to_string());
        for sr in &all_regs { if !sr.is_let { inits.push(format!("_{}(0)", sr.prefixed)); } }
        for prefix in &stage_prefixes { inits.push(format!("_{}_valid_r(0)", prefix)); }
        for stage_wires in &implicit_wires {
            for w in stage_wires { inits.push(format!("{}(0)", w.prefixed)); }
        }
        h.push_str(&format!("  {class}() : {} {{}}\n\n", inits.join(", ")));

        h.push_str("  void eval();\n  void eval_posedge();\n  void eval_comb();\n");
        h.push_str("  void _do_reset();\n  void final_() {}\n");
        h.push_str("  void trace_open(const char*) {}\n  void trace_dump(uint64_t) {}\n  void trace_close() {}\n\n");

        h.push_str("private:\n");
        h.push_str("  uint8_t _clk_prev;\n");
        for sr in &all_regs { if !sr.is_let { h.push_str(&format!("  {} _{};\n", sr.ty, sr.prefixed)); } }
        for prefix in &stage_prefixes { h.push_str(&format!("  uint8_t _{}_valid_r;\n", prefix)); }
        // Implicit wires (comb-block LHS targets and inst-output drivers).
        for stage_wires in &implicit_wires {
            for w in stage_wires {
                h.push_str(&format!("  {} {};\n", w.ty_cpp, w.prefixed));
            }
        }
        // Sub-instance members (one per inst inside any stage).
        for stage_list in &stage_insts {
            for inst in stage_list {
                h.push_str(&format!("  V{} _inst_{};\n", inst.module_name.name, inst.name.name));
            }
        }
        h.push_str("};\n");

        // ── Implementation ──
        let mut cpp = String::new();
        cpp.push_str(&format!("#include \"V{name}.h\"\n\n"));
        cpp.push_str(&format!("void {class}::eval() {{ eval_comb(); eval_posedge(); eval_comb(); }}\n\n"));

        // reset()
        cpp.push_str(&format!("void {class}::_do_reset() {{\n"));
        for sr in &all_regs {
            if !sr.is_let {
                let v = match sr.reset_val.as_str() { "false" | "1'b0" => "0", "true" | "1'b1" => "1", other => other };
                cpp.push_str(&format!("  _{} = {v};\n", sr.prefixed));
            }
        }
        for prefix in &stage_prefixes { cpp.push_str(&format!("  _{}_valid_r = 0;\n", prefix)); }
        cpp.push_str("}\n\n");

        // eval_posedge()
        cpp.push_str(&format!("void {class}::eval_posedge() {{\n"));
        cpp.push_str(&format!("  bool _rising = ({clk_name} && !_clk_prev);\n  _clk_prev = {clk_name};\n"));
        cpp.push_str("  if (_rising) {\n");
        cpp.push_str(&format!("    if ({rst_cond}) {{ _do_reset(); }} else {{\n"));

        // Evaluate let bindings first (they are combinational and may be referenced in seq blocks)
        for (si, stage) in p.stages.iter().enumerate() {
            let prefix = &stage_prefixes[si];
            for item in &stage.body {
                if let ModuleBodyItem::LetBinding(l) = item {
                    let val = self.pipeline_sim_expr(&l.value, prefix, si,
                        &stage_names, &stage_prefixes, &stage_reg_names,
                        &port_names, &reg_names, &let_names, &widths, &_enum_map);
                    let ty = if let Some(ref te) = l.ty { cpp_internal_type(te) } else { "uint32_t".to_string() };
                    let bits = if let Some(ref te) = l.ty { type_bits_te(te) } else { 32 };
                    let mask = if bits > 0 && bits < 64 { format!(" & 0x{:X}ULL", (1u64 << bits) - 1) } else { String::new() };
                    cpp.push_str(&format!("      {ty} {}_{} = ({val}){mask};\n", prefix, l.name.name));
                }
            }
        }

        // ── Stall signals ────────────────────────────────────────────────
        // Mirrors src/codegen/pipeline.rs: per-stage stall is the union of
        // its `stall when` condition, the pipeline-wide global stalls, and
        // any downstream stage's stall (back-pressure). Computed in reverse
        // stage order so each stage sees its downstream's resolved value.
        let has_per_stage_stall = p.stages.iter().any(|s| s.stall_cond.is_some());
        let has_global_stall = !p.stall_conds.is_empty();
        let has_any_stall = has_per_stage_stall || has_global_stall;
        if has_global_stall {
            let parts: Vec<String> = p.stall_conds.iter().map(|c| {
                self.pipeline_sim_expr(&c.condition, "", 0,
                    &stage_names, &stage_prefixes, &stage_reg_names,
                    &port_names, &reg_names, &let_names, &widths, &_enum_map)
            }).collect();
            cpp.push_str(&format!("      bool _pipeline_stall = ({});\n", parts.join(" || ")));
        }
        if has_any_stall {
            for si in (0..p.stages.len()).rev() {
                let prefix = &stage_prefixes[si];
                let mut parts: Vec<String> = Vec::new();
                if let Some(ref cond) = p.stages[si].stall_cond {
                    let s = self.pipeline_sim_expr(cond, prefix, si,
                        &stage_names, &stage_prefixes, &stage_reg_names,
                        &port_names, &reg_names, &let_names, &widths, &_enum_map);
                    parts.push(format!("({s})"));
                }
                if has_global_stall { parts.push("_pipeline_stall".to_string()); }
                if si + 1 < p.stages.len() {
                    parts.push(format!("_{}_stall", stage_prefixes[si + 1]));
                }
                let expr = if parts.is_empty() { "false".to_string() } else { parts.join(" || ") };
                cpp.push_str(&format!("      bool _{prefix}_stall = ({expr});\n"));
            }
        }

        // Process stages in reverse order so later stages read old values
        // (mimics SV non-blocking assignment semantics)
        for si in (0..p.stages.len()).rev() {
            let stage = &p.stages[si];
            let prefix = &stage_prefixes[si];
            // When stall is in play, this stage advances only if not stalled.
            // valid_r propagation: if upstream is stalled, insert a bubble.
            let (open_guard, close_guard, indent_extra) = if has_any_stall {
                (format!("      if (!_{prefix}_stall) {{\n"), "      }\n".to_string(), 2)
            } else {
                (String::new(), String::new(), 0)
            };
            cpp.push_str(&open_guard);
            let pad = " ".repeat(6 + indent_extra);
            if si == 0 {
                cpp.push_str(&format!("{pad}_{prefix}_valid_r = 1;\n"));
            } else {
                let prev = &stage_prefixes[si - 1];
                if has_any_stall {
                    // Bubble when prev stage is stalled (upstream can't deliver).
                    cpp.push_str(&format!("{pad}_{prefix}_valid_r = _{prev}_stall ? 0 : _{prev}_valid_r;\n"));
                } else {
                    cpp.push_str(&format!("{pad}_{prefix}_valid_r = _{prev}_valid_r;\n"));
                }
            }
            for item in &stage.body {
                if let ModuleBodyItem::RegBlock(rb) = item {
                    for stmt in &rb.stmts {
                        self.emit_pipeline_sim_stmt(&mut cpp, stmt, prefix, si,
                            &stage_names, &stage_prefixes, &stage_reg_names,
                            &port_names, &reg_names, &let_names, &widths, &_enum_map, 6 + indent_extra);
                    }
                }
            }
            cpp.push_str(&close_guard);
        }
        for flush in &p.flush_directives {
            let target = flush.target_stage.name.to_lowercase();
            let cond = self.pipeline_sim_expr(&flush.condition, "", 0,
                &stage_names, &stage_prefixes, &stage_reg_names,
                &port_names, &reg_names, &let_names, &widths, &_enum_map);
            cpp.push_str(&format!("      if ({cond}) {{ _{target}_valid_r = 0; }}\n"));
        }
        cpp.push_str("    }\n  }\n}\n\n");

        // eval_comb()
        cpp.push_str(&format!("void {class}::eval_comb() {{\n"));
        for (si, stage) in p.stages.iter().enumerate() {
            let prefix = &stage_prefixes[si];
            for item in &stage.body {
                match item {
                    ModuleBodyItem::LetBinding(l) => {
                        // ty=None means assignment to existing port/wire — skip as new binding
                        if l.ty.is_none() { continue; }
                        let val = self.pipeline_sim_expr(&l.value, prefix, si,
                            &stage_names, &stage_prefixes, &stage_reg_names,
                            &port_names, &reg_names, &let_names, &widths, &_enum_map);
                        let ty = if let Some(ref te) = l.ty { cpp_internal_type(te) } else { "uint32_t".to_string() };
                        let bits = if let Some(ref te) = l.ty { type_bits_te(te) } else { 32 };
                        let mask = if bits > 0 && bits < 64 { format!(" & 0x{:X}ULL", (1u64 << bits) - 1) } else { String::new() };
                        cpp.push_str(&format!("  {ty} {}_{} = ({val}){mask};\n", prefix, l.name.name));
                    }
                    ModuleBodyItem::CombBlock(cb) => {
                        for stmt in &cb.stmts {
                            self.emit_pipeline_sim_comb_stmt(&mut cpp, stmt, prefix, si,
                                &stage_names, &stage_prefixes, &stage_reg_names,
                                &port_names, &reg_names, &let_names, &widths, &_enum_map, 2);
                        }
                    }
                    ModuleBodyItem::Inst(inst) => {
                        let sub_ports = self.lookup_inst_ports(&inst.module_name.name);
                        // Inputs first.
                        for conn in &inst.connections {
                            if conn.direction != ConnectDir::Input { continue; }
                            let val = self.pipeline_sim_expr(&conn.signal, prefix, si,
                                &stage_names, &stage_prefixes, &stage_reg_names,
                                &port_names, &reg_names, &let_names, &widths, &_enum_map);
                            cpp.push_str(&format!("  _inst_{}.{} = {val};\n",
                                inst.name.name, conn.port_name.name));
                        }
                        // Eval the sub-instance.
                        cpp.push_str(&format!("  _inst_{}.eval();\n", inst.name.name));
                        // Outputs to driver wires.
                        for conn in &inst.connections {
                            if conn.direction != ConnectDir::Output { continue; }
                            let ExprKind::Ident(target) = &conn.signal.kind else { continue; };
                            let lhs = if port_names.contains(target) {
                                target.clone()
                            } else {
                                format!("{}_{}", prefix, target)
                            };
                            // Mask to match the implicit-wire width when narrower than 64.
                            let bits = sub_ports.iter().find(|p| p.name.name == conn.port_name.name)
                                .map(|p| type_bits_te(&p.ty)).unwrap_or(32);
                            let mask = if bits > 0 && bits < 64 {
                                format!(" & 0x{:X}ULL", (1u64 << bits) - 1)
                            } else { String::new() };
                            cpp.push_str(&format!("  {lhs} = (_inst_{}.{}){mask};\n",
                                inst.name.name, conn.port_name.name));
                        }
                    }
                    _ => {}
                }
            }
        }
        cpp.push_str("}\n");

        SimModel { class_name: class, header: h, impl_: cpp }
    }

    fn emit_pipeline_sim_stmt(
        &self, cpp: &mut String, stmt: &Stmt, prefix: &str, si: usize,
        sn: &[String], sp: &[String], srn: &[HashSet<String>],
        pn: &HashSet<String>, rn: &HashSet<String>, ln: &HashSet<String>,
        w: &HashMap<String, u32>, em: &HashMap<String, Vec<(String, u64)>>, indent: usize,
    ) {
        let pad = " ".repeat(indent);
        match stmt {
            Stmt::Assign(a) => {
                let tgt = if let ExprKind::Ident(n) = &a.target.kind {
                    if pn.contains(n) { n.clone() } else { format!("_{}_{}", prefix, n) }
                } else { self.pipeline_sim_expr(&a.target, prefix, si, sn, sp, srn, pn, rn, ln, w, em) };
                let val = self.pipeline_sim_expr(&a.value, prefix, si, sn, sp, srn, pn, rn, ln, w, em);
                let tgt_key = if let ExprKind::Ident(n) = &a.target.kind { format!("{}_{}", prefix, n) } else { String::new() };
                let mask = w.get(&tgt_key).and_then(|&b| if b > 0 && b < 64 { Some(format!(" & 0x{:X}ULL", (1u64 << b) - 1)) } else { None }).unwrap_or_default();
                cpp.push_str(&format!("{pad}{tgt} = ({val}){mask};\n"));
            }
            Stmt::IfElse(ie) => {
                let cond = self.pipeline_sim_expr(&ie.cond, prefix, si, sn, sp, srn, pn, rn, ln, w, em);
                cpp.push_str(&format!("{pad}if ({cond}) {{\n"));
                for s in &ie.then_stmts { self.emit_pipeline_sim_stmt(cpp, s, prefix, si, sn, sp, srn, pn, rn, ln, w, em, indent+2); }
                if !ie.else_stmts.is_empty() {
                    cpp.push_str(&format!("{pad}}} else {{\n"));
                    for s in &ie.else_stmts { self.emit_pipeline_sim_stmt(cpp, s, prefix, si, sn, sp, srn, pn, rn, ln, w, em, indent+2); }
                }
                cpp.push_str(&format!("{pad}}}\n"));
            }
            Stmt::For(f) => {
                if let ForRange::Range(ref lo_expr, ref hi_expr) = f.range {
                    let lo = self.pipeline_sim_expr(lo_expr, prefix, si, sn, sp, srn, pn, rn, ln, w, em);
                    let hi = self.pipeline_sim_expr(hi_expr, prefix, si, sn, sp, srn, pn, rn, ln, w, em);
                    cpp.push_str(&format!("{pad}for (int {v} = {lo}; {v} <= {hi}; {v}++) {{\n", v=f.var.name));
                    for s in &f.body { self.emit_pipeline_sim_stmt(cpp, s, prefix, si, sn, sp, srn, pn, rn, ln, w, em, indent+2); }
                    cpp.push_str(&format!("{pad}}}\n"));
                }
            }
            _ => {}
        }
    }

    fn emit_pipeline_sim_comb_stmt(
        &self, cpp: &mut String, stmt: &Stmt, prefix: &str, si: usize,
        sn: &[String], sp: &[String], srn: &[HashSet<String>],
        pn: &HashSet<String>, rn: &HashSet<String>, ln: &HashSet<String>,
        w: &HashMap<String, u32>, em: &HashMap<String, Vec<(String, u64)>>, indent: usize,
    ) {
        let pad = " ".repeat(indent);
        match stmt {
            Stmt::Assign(a) => {
                let tgt = if let ExprKind::Ident(n) = &a.target.kind {
                    if pn.contains(n) { n.clone() } else { format!("{}_{}", prefix, n) }
                } else { self.pipeline_sim_expr(&a.target, prefix, si, sn, sp, srn, pn, rn, ln, w, em) };
                let val = self.pipeline_sim_expr(&a.value, prefix, si, sn, sp, srn, pn, rn, ln, w, em);
                cpp.push_str(&format!("{pad}{tgt} = {val};\n"));
            }
            Stmt::IfElse(ie) => {
                let cond = self.pipeline_sim_expr(&ie.cond, prefix, si, sn, sp, srn, pn, rn, ln, w, em);
                cpp.push_str(&format!("{pad}if ({cond}) {{\n"));
                for s in &ie.then_stmts { self.emit_pipeline_sim_comb_stmt(cpp, s, prefix, si, sn, sp, srn, pn, rn, ln, w, em, indent+2); }
                if !ie.else_stmts.is_empty() {
                    cpp.push_str(&format!("{pad}}} else {{\n"));
                    for s in &ie.else_stmts { self.emit_pipeline_sim_comb_stmt(cpp, s, prefix, si, sn, sp, srn, pn, rn, ln, w, em, indent+2); }
                }
                cpp.push_str(&format!("{pad}}}\n"));
            }
            _ => {}
        }
    }

    fn pipeline_sim_expr(
        &self, expr: &Expr, prefix: &str, si: usize,
        sn: &[String], sp: &[String], srn: &[HashSet<String>],
        pn: &HashSet<String>, rn: &HashSet<String>, ln: &HashSet<String>,
        w: &HashMap<String, u32>, em: &HashMap<String, Vec<(String, u64)>>,
    ) -> String {
        let empty = HashSet::new();
        let empty_rl: HashMap<String, ResetLevel> = HashMap::new();
        let ctx = Ctx { reg_names: rn, port_names: pn, let_names: ln, inst_names: &empty, wide_names: &empty, widths: w, posedge_lhs: false, fsm_mode: false, enum_map: em, bus_ports: &empty, reset_levels: &empty_rl, vec_names: None, vec_sizes: None, fsm_vec_port_regs: None, ident_subst: None, coverage: None, params: &[] };
        match &expr.kind {
            ExprKind::FieldAccess(base, field) => {
                if let ExprKind::Ident(bn) = &base.kind {
                    if let Some(idx) = sn.iter().position(|s| s == bn) {
                        let p = &sp[idx];
                        let prefixed = format!("{}_{}", p, field.name);
                        if rn.contains(&prefixed) { return format!("_{}", prefixed); }
                        if ln.contains(&prefixed) { return prefixed; }
                        if field.name == "valid_r" { return format!("_{}_valid_r", p); }
                        return format!("_{}", prefixed);
                    }
                }
                cpp_expr(expr, &ctx)
            }
            ExprKind::Ident(name) => {
                if pn.contains(name) { return name.clone(); }
                if name == "valid_r" && !prefix.is_empty() { return format!("_{}_valid_r", prefix); }
                if si < srn.len() && srn[si].contains(name) {
                    let prefixed = format!("{}_{}", prefix, name);
                    if rn.contains(&prefixed) { return format!("_{}", prefixed); }
                    if ln.contains(&prefixed) { return prefixed; }
                }
                cpp_expr(expr, &ctx)
            }
            ExprKind::Concat(parts) => {
                let mut total_bits: u32 = 0;
                let part_widths: Vec<u32> = parts.iter().map(|p2| {
                    let pw = self.pipeline_sim_expr_width(p2, prefix, si, srn, w, pn);
                    total_bits += pw;
                    pw
                }).collect();
                let _ = total_bits; // used implicitly
                let mut result = String::from("(");
                let mut bit_pos: u32 = part_widths.iter().sum();
                for (i, part) in parts.iter().enumerate() {
                    bit_pos -= part_widths[i];
                    let val = self.pipeline_sim_expr(part, prefix, si, sn, sp, srn, pn, rn, ln, w, em);
                    if i > 0 { result.push_str(" | "); }
                    if bit_pos > 0 { result.push_str(&format!("((uint64_t)({val}) << {bit_pos})")); }
                    else { result.push_str(&format!("(uint64_t)({val})")); }
                }
                result.push(')');
                result
            }
            ExprKind::Binary(op, lhs, rhs) => {
                let l = self.pipeline_sim_expr(lhs, prefix, si, sn, sp, srn, pn, rn, ln, w, em);
                let r = self.pipeline_sim_expr(rhs, prefix, si, sn, sp, srn, pn, rn, ln, w, em);
                if matches!(*op, BinOp::Implies | BinOp::ImpliesNext) {
                    return format!("(!{l} || {r})");
                }
                let os = match op {
                    BinOp::Add | BinOp::AddWrap => "+", BinOp::Sub | BinOp::SubWrap => "-",
                    BinOp::Mul | BinOp::MulWrap => "*", BinOp::Div => "/", BinOp::Mod => "%",
                    BinOp::Eq => "==", BinOp::Neq => "!=", BinOp::Lt => "<", BinOp::Gt => ">",
                    BinOp::Lte => "<=", BinOp::Gte => ">=", BinOp::And => "&&", BinOp::Or => "||",
                    BinOp::BitAnd => "&", BinOp::BitOr => "|", BinOp::BitXor => "^", BinOp::Shl => "<<", BinOp::Shr => ">>",
                    BinOp::Implies | BinOp::ImpliesNext => unreachable!(),
                };
                format!("({l} {os} {r})")
            }
            ExprKind::Unary(op, inner) => {
                let v = self.pipeline_sim_expr(inner, prefix, si, sn, sp, srn, pn, rn, ln, w, em);
                match op { UnaryOp::Not => format!("(!{v})"), UnaryOp::BitNot => format!("(~{v})"), UnaryOp::Neg => format!("(-{v})"),
                    UnaryOp::RedAnd | UnaryOp::RedOr | UnaryOp::RedXor => format!("({v})") }
            }
            ExprKind::MethodCall(base, method, args) => {
                let b = self.pipeline_sim_expr(base, prefix, si, sn, sp, srn, pn, rn, ln, w, em);
                match method.name.as_str() {
                    "trunc" => { if let Some(wa) = args.first() { let bits = eval_const_expr(wa); if bits < 64 { format!("({b} & 0x{:X}ULL)", (1u64 << bits) - 1) } else { b } } else { b } }
                    "zext" => { format!("(uint64_t)({b})") }
                    "sext" => { b } // TODO: proper sign extension
                    _ => b
                }
            }
            ExprKind::BitSlice(base, hi, lo) => {
                let b = self.pipeline_sim_expr(base, prefix, si, sn, sp, srn, pn, rn, ln, w, em);
                let hv = eval_const_expr(hi); let lv = eval_const_expr(lo); let width = hv - lv + 1;
                if width < 64 { format!("(({b} >> {lv}) & 0x{:X}ULL)", (1u64 << width) - 1) } else { format!("({b} >> {lv})") }
            }
            ExprKind::Index(base, idx) => {
                let b = self.pipeline_sim_expr(base, prefix, si, sn, sp, srn, pn, rn, ln, w, em);
                let i = self.pipeline_sim_expr(idx, prefix, si, sn, sp, srn, pn, rn, ln, w, em);
                format!("(({b} >> {i}) & 1)")
            }
            ExprKind::Literal(lit) => match lit { LitKind::Dec(v) => format!("{v}"), LitKind::Hex(v) | LitKind::Bin(v) => format!("0x{v:X}"), LitKind::Sized(_, v) => format!("{v}") },
            ExprKind::Bool(b) => if *b { "1".to_string() } else { "0".to_string() },
            ExprKind::Ternary(c, t, e) => {
                let cv = self.pipeline_sim_expr(c, prefix, si, sn, sp, srn, pn, rn, ln, w, em);
                let tv = self.pipeline_sim_expr(t, prefix, si, sn, sp, srn, pn, rn, ln, w, em);
                let ev = self.pipeline_sim_expr(e, prefix, si, sn, sp, srn, pn, rn, ln, w, em);
                format!("(({cv}) ? ({tv}) : ({ev}))")
            }
            ExprKind::Signed(inner) | ExprKind::Unsigned(inner) | ExprKind::Cast(inner, _) => {
                self.pipeline_sim_expr(inner, prefix, si, sn, sp, srn, pn, rn, ln, w, em)
            }
            ExprKind::Clog2(arg) => { let a = self.pipeline_sim_expr(arg, prefix, si, sn, sp, srn, pn, rn, ln, w, em); format!("_arch_clog2({a})") }
            _ => cpp_expr(expr, &ctx),
        }
    }

    fn pipeline_sim_expr_width(&self, expr: &Expr, prefix: &str, si: usize, srn: &[HashSet<String>], w: &HashMap<String, u32>, pn: &HashSet<String>) -> u32 {
        match &expr.kind {
            ExprKind::Ident(name) => {
                if pn.contains(name) { return *w.get(name).unwrap_or(&8); }
                if si < srn.len() && srn[si].contains(name) { return *w.get(&format!("{}_{}", prefix, name)).unwrap_or(&8); }
                *w.get(name).unwrap_or(&8)
            }
            ExprKind::FieldAccess(base, field) => {
                if let ExprKind::Ident(bn) = &base.kind { *w.get(&format!("{}_{}", bn.to_lowercase(), field.name)).unwrap_or(&8) } else { 8 }
            }
            ExprKind::MethodCall(_, method, args) => match method.name.as_str() { "trunc"|"zext"|"sext"|"resize" => args.first().map(|a| eval_const_expr(a) as u32).unwrap_or(8), _ => 8 },
            ExprKind::BitSlice(_, hi, lo) => { let h = eval_const_expr(hi); let l = eval_const_expr(lo); (h - l + 1) as u32 }
            ExprKind::Literal(LitKind::Sized(ww, _)) => *ww,
            _ => 8,
        }
    }

    fn pipeline_reset_value(reset: &RegReset) -> Option<String> {
        match reset {
            RegReset::Explicit(_, _, _, val) | RegReset::Inherit(_, val) => {
                match &val.kind {
                    ExprKind::Literal(LitKind::Dec(v)) => Some(format!("{v}")),
                    ExprKind::Literal(LitKind::Hex(v)) => Some(format!("0x{v:X}")),
                    ExprKind::Bool(b) => Some(if *b { "1".to_string() } else { "0".to_string() }),
                    _ => Some("0".to_string()),
                }
            }
            _ => Some("0".to_string()),
        }
    }
}
