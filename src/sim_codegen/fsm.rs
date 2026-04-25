//! `gen_fsm` emitter — extracted from sim_codegen/mod.rs.
//!
//! Lives in a submodule of `sim_codegen` so the `super::` scope keeps
//! visibility of the shared free-function helpers (`build_enum_map`,
//! `cpp_internal_type`, etc.) without needing to bump each to `pub(crate)`.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use super::{SimCodegen, SimModel};
use super::*;

impl<'a> SimCodegen<'a> {
    pub(super) fn gen_fsm(&self, f: &FsmDecl) -> SimModel {
        let name = &f.name.name;
        let class = format!("V{name}");
        let enum_map = build_enum_map(self.symbols);

        // --coverage phase 3: per-state and per-transition counter
        // registry. Same pattern as gen_module: emit `_arch_cov[N]++;`
        // at each state's case body (state-entry coverage) and at each
        // transition's `if (cond) ...` (transition-arc coverage).
        let cov_reg: std::cell::RefCell<CoverageRegistry> = std::cell::RefCell::new(CoverageRegistry::default());
        let cov_handle: Option<&std::cell::RefCell<CoverageRegistry>> =
            if self.coverage { Some(&cov_reg) } else { None };

        // Collect bus port names and flattened signals (same pattern as gen_module)
        let mut bus_port_names: HashSet<String> = HashSet::new();
        let mut bus_flat: Vec<(String, TypeExpr)> = Vec::new();
        for p in &f.ports {
            if let Some(ref bi) = p.bus_info {
                bus_port_names.insert(p.name.name.clone());
                bus_flat.extend(flatten_bus_port(&p.name.name, bi, self.symbols));
            }
        }

        let mut port_names: HashSet<String> = f.ports.iter()
            .filter(|p| p.bus_info.is_none())
            .map(|p| p.name.name.clone())
            .collect();
        for (flat_name, _) in &bus_flat {
            port_names.insert(flat_name.clone());
        }

        let empty_regs  = HashSet::new();
        let empty_lets  = HashSet::new();
        let empty_insts = HashSet::new();
        let empty_wide  = HashSet::new();
        let empty_w     = HashMap::new();

        let n_states   = f.state_names.len();
        let state_bits = enum_width(n_states);
        let state_ty   = cpp_uint(state_bits as u32);

        let state_idx: HashMap<String, usize> = f.state_names.iter()
            .enumerate().map(|(i, s)| (s.name.clone(), i)).collect();
        let default_idx = state_idx.get(&f.default_state.name).copied().unwrap_or(0);

        let (rst_name, _is_async, is_low) = extract_reset_info(&f.ports);
        let rst_cond = if is_low { format!("(!{})", rst_name) } else { rst_name.clone() };

        let mut h = String::new();
        h.push_str("#pragma once\n#include <cstdint>\n#include <cstdio>\n#include \"verilated.h\"\n\n");
        // Emit param constants as #define
        for p in &f.params {
            if matches!(p.kind, ParamKind::Const | ParamKind::WidthConst(..)) {
                if let Some(ref def) = p.default {
                    let val = eval_const_expr(def);
                    h.push_str(&format!("#ifndef {}\n#define {} {val}ULL\n#endif\n", p.name.name, p.name.name));
                }
            }
        }
        // Collect FSM Vec port infos (for flat field emission and internal arrays)
        struct FsmVecPortInfo {
            name: String,
            elem_ty: String,
            count: u64,
            is_input: bool,
            is_port_reg: bool,
        }
        let fsm_vec_port_infos: Vec<FsmVecPortInfo> = f.ports.iter()
            .filter_map(|p| {
                if let Some((elem_ty, count_str)) = vec_array_info(&p.ty) {
                    let count: u64 = count_str.parse().unwrap_or(0);
                    Some(FsmVecPortInfo {
                        name: p.name.name.clone(),
                        elem_ty,
                        count,
                        is_input: p.direction == Direction::In,
                        is_port_reg: p.reg_info.is_some(),
                    })
                } else {
                    None
                }
            })
            .collect();
        let fsm_vec_port_names: HashSet<String> = fsm_vec_port_infos.iter().map(|v| v.name.clone()).collect();
        // All FSM Vec ports have internal C arrays `_name[N]` and always resolve to `_name` in ctx.
        // (Both input and output Vec ports, whether port-reg or not.)
        let fsm_vec_port_reg_names: HashSet<String> = fsm_vec_port_names.clone();
        // Vec-typed regs in f.regs also need array subscript in Index expressions.
        let fsm_vec_reg_names: HashSet<String> = f.regs.iter()
            .filter(|r| matches!(r.ty, TypeExpr::Vec(..)))
            .map(|r| r.name.name.clone())
            .collect();
        // All Vec names for the ctx (so Index uses `[i]` syntax): ports + regs
        let mut fsm_vec_names: HashSet<String> = fsm_vec_port_names.clone();
        fsm_vec_names.extend(fsm_vec_reg_names.iter().cloned());

        h.push('\n');
        h.push_str(&format!("class {class} {{\npublic:\n  // State constants\n"));
        for (i, sn) in f.state_names.iter().enumerate() {
            h.push_str(&format!("  static const {state_ty} STATE_{} = {i};\n", sn.name.to_uppercase()));
        }
        h.push('\n');
        // Port fields: Vec ports → N flat fields; bus ports → flattened signals; others → single field
        for p in &f.ports {
            if bus_port_names.contains(&p.name.name) {
                continue; // bus ports emitted separately as flattened signals
            } else if let Some(vi) = fsm_vec_port_infos.iter().find(|v| v.name == p.name.name) {
                for i in 0..vi.count {
                    h.push_str(&format!("  {} {}_{i};\n", vi.elem_ty, vi.name));
                }
            } else {
                h.push_str(&format!("  {} {};\n", cpp_port_type(&p.ty), p.name.name));
            }
        }
        // Flattened bus port fields
        for (flat_name, flat_ty) in &bus_flat {
            let ty = cpp_port_type(flat_ty);
            h.push_str(&format!("  {ty} {flat_name};\n"));
        }
        // Datapath registers as public members (accessible from testbench)
        for reg in &f.regs {
            if let Some((elem_ty, count)) = vec_array_info(&reg.ty) {
                h.push_str(&format!("  {} {}[{}];\n", elem_ty, reg.name.name, count));
            } else {
                let ty = cpp_internal_type(&reg.ty);
                h.push_str(&format!("  {} {};\n", ty, reg.name.name));
            }
        }
        // Let bindings as public members
        for lb in &f.lets {
            let ty = lb.ty.as_ref().map(|t| cpp_internal_type(t)).unwrap_or_else(|| "uint32_t".to_string());
            h.push_str(&format!("  {} {};\n", ty, lb.name.name));
        }
        // Wire declarations as public members
        for w in &f.wires {
            let ty = cpp_internal_type(&w.ty);
            h.push_str(&format!("  {} {};\n", ty, w.name.name));
        }
        h.push('\n');

        // Constructor inits: skip Vec/bus port names (they use flat fields), add flat field inits
        let port_inits: Vec<String> = f.ports.iter()
            .filter(|p| !fsm_vec_port_names.contains(&p.name.name) && !bus_port_names.contains(&p.name.name))
            .map(|p| format!("{}(0)", p.name.name))
            .collect();
        let vec_port_flat_inits: Vec<String> = fsm_vec_port_infos.iter()
            .flat_map(|vi| (0..vi.count).map(move |i| format!("{}_{i}(0)", vi.name)))
            .collect();
        let bus_flat_inits: Vec<String> = bus_flat.iter()
            .map(|(n, _)| format!("{n}(0)"))
            .collect();
        let reg_inits: Vec<String> = f.regs.iter()
            .filter(|r| !matches!(r.ty, TypeExpr::Vec(..)))  // Vec regs use memset in ctor body
            .map(|r| {
                let init_expr = reset_value_from_reg_reset(&r.reset)
                    .or(r.init.as_ref());
                if let Some(expr) = init_expr {
                    let init_val = cpp_expr(expr, &Ctx::new(&empty_regs, &port_names, &empty_lets, &empty_insts, &empty_wide, &empty_w, &enum_map, &bus_port_names));
                    format!("{}({})", r.name.name, init_val)
                } else {
                    format!("{}(0)", r.name.name)
                }
            }).collect();
        let state_inits = vec!["_clk_prev(0)".to_string(), format!("_state_r({default_idx})")];
        let all_inits: Vec<String> = port_inits.into_iter()
            .chain(vec_port_flat_inits)
            .chain(bus_flat_inits)
            .chain(reg_inits)
            .chain(state_inits)
            .collect();
        // Constructor body: memset internal arrays for Vec ports + Vec regs
        let mut fsm_vec_memsets: Vec<String> = fsm_vec_port_infos.iter()
            .map(|vi| format!("    memset(_{}, 0, sizeof(_{}));", vi.name, vi.name))
            .collect();
        // Vec regs in FSM: public array members, initialized via memset
        for reg in &f.regs {
            if matches!(reg.ty, TypeExpr::Vec(..)) {
                let n = &reg.name.name;
                fsm_vec_memsets.push(format!("    memset({n}, 0, sizeof({n}));"));
            }
        }
        if fsm_vec_memsets.is_empty() {
            h.push_str(&format!("  {class}() : {} {{}}\n", all_inits.join(", ")));
        } else {
            h.push_str(&format!("  {class}() : {} {{\n", all_inits.join(", ")));
            for ms in &fsm_vec_memsets { h.push_str(&format!("{ms}\n")); }
            h.push_str("  }\n");
        }
        h.push_str(&format!("  explicit {class}(VerilatedContext*) : {class}() {{}}\n"));
        h.push_str("  void eval();\n  void eval_posedge();\n  void eval_comb();\n  void final() { trace_close(); }\nprivate:\n");
        // Private internal arrays for Vec ports
        for vi in &fsm_vec_port_infos {
            h.push_str(&format!("  {} _{}[{}];\n", vi.elem_ty, vi.name, vi.count));
        }
        h.push_str("  uint8_t _clk_prev;\n");
        h.push_str(&format!("  {state_ty} _state_r;\n"));
        // };\n deferred until after trace support added

        let mut cpp = String::new();
        cpp.push_str(&format!("#include \"{class}.h\"\n\n"));

        let clk_port = f.ports.iter().find(|p| matches!(&p.ty, TypeExpr::Clock(_)))
            .map(|p| p.name.name.as_str()).unwrap_or("clk");

        cpp.push_str(&format!("void {class}::eval() {{\n"));
        cpp.push_str("  if (!_trace_fp && Verilated::traceFile() && Verilated::claimTrace())\n");
        cpp.push_str("    trace_open(Verilated::traceFile());\n");
        cpp.push_str("  eval_comb();\n  eval_posedge();\n  eval_comb();\n");
        cpp.push_str("  if (_trace_fp) trace_dump(_trace_time++);\n");
        cpp.push_str("}\n\n");

        let fsm_reg_names: HashSet<String> = f.regs.iter().map(|r| r.name.name.clone()).collect();
        let fsm_let_names: HashSet<String> = f.lets.iter().map(|l| l.name.name.clone())
            .chain(f.wires.iter().map(|w| w.name.name.clone())).collect();
        let mut fsm_widths: HashMap<String, u32> = HashMap::new();
        for p in &f.ports { fsm_widths.insert(p.name.name.clone(), type_bits_te(&p.ty)); }
        for r in &f.regs { fsm_widths.insert(r.name.name.clone(), type_bits_te(&r.ty)); }
        for l in &f.lets {
            if let Some(ty) = &l.ty { fsm_widths.insert(l.name.name.clone(), type_bits_te(ty)); }
        }
        for w in &f.wires { fsm_widths.insert(w.name.name.clone(), type_bits_te(&w.ty)); }
        let ctx_fsm = {
            let mut c = Ctx::new(&fsm_reg_names, &port_names, &fsm_let_names, &empty_insts,
                                 &empty_wide, &fsm_widths, &enum_map, &bus_port_names)
                .with_vec_names(&fsm_vec_names)
                .with_fsm_vec_port_regs(&fsm_vec_port_reg_names);
            c.fsm_mode = true;
            c
        };

        cpp.push_str(&format!("void {class}::eval_posedge() {{\n"));
        cpp.push_str(&format!("  bool _rising = ({clk_port} && !_clk_prev);\n"));
        cpp.push_str(&format!("  _clk_prev = {clk_port};\n"));
        cpp.push_str("  if (!_rising) return;\n");
        cpp.push_str(&format!("  {state_ty} _n_state = _state_r;\n"));
        // Shadow variables for datapath regs
        for reg in &f.regs {
            let n = &reg.name.name;
            if let Some((elem_ty, count)) = vec_array_info(&reg.ty) {
                cpp.push_str(&format!("  {elem_ty} _n_{n}[{count}]; memcpy(_n_{n}, {n}, sizeof({n}));\n"));
            } else {
                let ty = cpp_internal_type(&reg.ty);
                cpp.push_str(&format!("  {ty} _n_{n} = {n};\n"));
            }
        }
        cpp.push_str(&format!("  if ({rst_cond}) {{\n    _n_state = {default_idx};\n"));
        // Reset datapath regs
        for reg in &f.regs {
            let reset_expr = reset_value_from_reg_reset(&reg.reset)
                .or(reg.init.as_ref());
            if let Some(expr) = reset_expr {
                let n = &reg.name.name;
                if vec_array_info(&reg.ty).is_some() {
                    let init_val = cpp_expr(expr, &ctx_fsm);
                    let count = if let TypeExpr::Vec(_, c) = &reg.ty { eval_const_expr(c) } else { 0 };
                    cpp.push_str(&format!("    for (int _i = 0; _i < {count}; _i++) _n_{n}[_i] = {init_val};\n"));
                } else {
                    let init_val = cpp_expr(expr, &ctx_fsm);
                    cpp.push_str(&format!("    _n_{n} = {init_val};\n"));
                }
            }
        }
        // Reset Vec port-regs
        for vi in &fsm_vec_port_infos {
            if vi.is_port_reg {
                let p = f.ports.iter().find(|p| p.name.name == vi.name).unwrap();
                let reset_expr = p.reg_info.as_ref().and_then(|ri| reset_value_from_reg_reset(&ri.reset).or(ri.init.as_ref()));
                let reset_val = if let Some(expr) = reset_expr {
                    cpp_expr(expr, &ctx_fsm)
                } else {
                    "0".to_string()
                };
                cpp.push_str(&format!("    for (int _i = 0; _i < {}; _i++) _{}[_i] = {};\n",
                    vi.count, vi.name, reset_val));
            }
        }
        cpp.push_str("  } else {\n");
        let ctx_posedge = {
            let mut c = Ctx::new(&fsm_reg_names, &port_names, &fsm_let_names, &empty_insts,
                                 &empty_wide, &fsm_widths, &enum_map, &bus_port_names)
                .with_vec_names(&fsm_vec_names)
                .with_fsm_vec_port_regs(&fsm_vec_port_reg_names);
            c.posedge_lhs = true;
            c.fsm_mode = true;
            c
        };
        // Default sequential assignments
        for stmt in &f.default_seq {
            let mut body = String::new();
            emit_reg_stmt(stmt, &ctx_posedge, &mut body, 2);
            cpp.push_str(&body);
        }
        cpp.push_str("    switch (_state_r) {\n");
        for sb in &f.states {
            let idx = state_idx.get(&sb.name.name).copied().unwrap_or(0);
            cpp.push_str(&format!("      case {idx}: // {}\n", sb.name.name));
            // --coverage: state-entry counter (per posedge that
            // dispatched into this state's case). 0 hits means the
            // state was never entered — useful for unreachable-state
            // diagnostics.
            if let Some(reg) = cov_handle {
                let cidx = reg.borrow_mut().alloc("state", sb.name.span.start, format!("state {}", sb.name.name));
                cpp.push_str(&format!("        _arch_cov[{cidx}]++;\n"));
            }
            // Emit seq_stmts for this state
            for stmt in &sb.seq_stmts {
                let mut body = String::new();
                emit_reg_stmt(stmt, &ctx_posedge, &mut body, 4);
                cpp.push_str(&body);
            }
            for tr in &sb.transitions {
                let cond = cpp_expr(&tr.condition, &ctx_fsm);
                let target_idx = state_idx.get(&tr.target.name).copied().unwrap_or(0);
                // --coverage: per-transition counter. The bump is
                // inside the `if (cond) { ... }` so it only increments
                // when the arc is actually taken (not on every
                // condition evaluation).
                let cov_bump = if let Some(reg) = cov_handle {
                    let cidx = reg.borrow_mut().alloc(
                        "trans",
                        tr.condition.span.start,
                        format!("trans {} -> {}", sb.name.name, tr.target.name),
                    );
                    format!("_arch_cov[{cidx}]++; ")
                } else { String::new() };
                if self.debug_fsm {
                    // Escape the condition for printf literal
                    let cond_escaped = cond.replace('\\', "\\\\").replace('"', "\\\"").replace('%', "%%");
                    cpp.push_str(&format!(
                        "        if ({cond}) {{ {cov_bump}_n_state = {target_idx}; \
                         printf(\"[FSM][{name}] {src} -> {tgt} ({cond_lit})\\n\"); break; }}\n",
                        src = sb.name.name,
                        tgt = tr.target.name,
                        cond_lit = cond_escaped,
                    ));
                } else {
                    cpp.push_str(&format!("        if ({cond}) {{ {cov_bump}_n_state = {target_idx}; break; }}\n"));
                }
            }
            cpp.push_str("        break;\n");
        }
        cpp.push_str("    }\n  }\n  _state_r = _n_state;\n");
        // Commit datapath regs
        for reg in &f.regs {
            let n = &reg.name.name;
            if vec_array_info(&reg.ty).is_some() {
                cpp.push_str(&format!("  memcpy({n}, _n_{n}, sizeof({n}));\n"));
            } else {
                cpp.push_str(&format!("  {n} = _n_{n};\n"));
            }
        }
        // Fan out Vec port-reg internal arrays to flat public fields
        for vi in &fsm_vec_port_infos {
            if vi.is_port_reg {
                for i in 0..vi.count {
                    cpp.push_str(&format!("  {}_{i} = _{}[{i}];\n", vi.name, vi.name));
                }
            }
        }
        cpp.push_str("}\n\n");

        cpp.push_str(&format!("void {class}::eval_comb() {{\n"));
        // Flat → internal bridge for input Vec ports
        for vi in &fsm_vec_port_infos {
            if vi.is_input && !vi.is_port_reg {
                for i in 0..vi.count {
                    cpp.push_str(&format!("  _{}[{i}] = {}_{i};\n", vi.name, vi.name));
                }
            }
        }
        // Let bindings
        for lb in &f.lets {
            let val = cpp_expr(&lb.value, &ctx_fsm);
            cpp.push_str(&format!("  {} = {};\n", lb.name.name, val));
        }
        // Default combinational assignments
        {
            let mut body = String::new();
            emit_comb_stmts(&f.default_comb, &ctx_fsm, &mut body, 1);
            cpp.push_str(&body);
        }
        cpp.push_str("  switch (_state_r) {\n");
        for sb in &f.states {
            let idx = state_idx.get(&sb.name.name).copied().unwrap_or(0);
            cpp.push_str(&format!("    case {idx}: {{ // {}\n", sb.name.name));
            let mut body = String::new();
            emit_comb_stmts(&sb.comb_stmts, &ctx_fsm, &mut body, 3);
            cpp.push_str(&body);
            cpp.push_str("      break;\n    }\n");
        }
        cpp.push_str("  }\n");
        // Internal → flat bridge for output Vec ports (non-reg)
        for vi in &fsm_vec_port_infos {
            if !vi.is_input && !vi.is_port_reg {
                for i in 0..vi.count {
                    cpp.push_str(&format!("  {}_{i} = _{}[{i}];\n", vi.name, vi.name));
                }
            }
        }
        cpp.push_str("}\n");

        // Add trace support
        // Build flat Vec port trace signals (name_i → field name_i, width = elem_width)
        let mut fsm_flat_vec_traces: Vec<(String, String, u32)> = Vec::new();
        for vi in &fsm_vec_port_infos {
            let elem_bits = if let TypeExpr::Vec(elem, _) = f.ports.iter()
                .find(|p| p.name.name == vi.name).map(|p| &p.ty).unwrap() {
                type_width(elem)
            } else { 32 };
            for i in 0..vi.count {
                let fname = format!("{}_{i}", vi.name);
                fsm_flat_vec_traces.push((fname.clone(), fname, elem_bits));
            }
        }
        let mut extra_sigs_owned: Vec<(String, String, u32)> = vec![
            ("state_r".to_string(), "_state_r".to_string(), state_bits as u32),
        ];
        extra_sigs_owned.extend(fsm_flat_vec_traces);
        // Add flattened bus port signals to trace
        for (flat_name, flat_ty) in &bus_flat {
            let bits = type_bits_te(flat_ty);
            extra_sigs_owned.push((flat_name.clone(), flat_name.clone(), bits));
        }
        let extra_sigs_ref: Vec<(&str, &str, u32)> = extra_sigs_owned.iter()
            .map(|(n, e, w)| (n.as_str(), e.as_str(), *w))
            .collect();
        add_trace_to_simple_construct(&mut h, &mut cpp, &class, name, &f.ports, &extra_sigs_ref);

        // --coverage: per-FSM counter storage + atexit dumper. Same
        // shape as gen_module's coverage emission (#132/#134).
        let n_cov = cov_reg.borrow().points.len();
        if self.coverage && n_cov > 0 {
            h.push_str(&format!("public:\n  static uint64_t _arch_cov[{n_cov}];\n  static bool _arch_cov_dumped;\n"));
            cpp.push_str(&format!("uint64_t {class}::_arch_cov[{n_cov}] = {{}};\nbool {class}::_arch_cov_dumped = false;\n\n"));
            cpp.push_str("namespace {\n");
            cpp.push_str("static void _arch_cov_dump() {\n");
            cpp.push_str(&format!("  if ({class}::_arch_cov_dumped) return;\n"));
            cpp.push_str(&format!("  {class}::_arch_cov_dumped = true;\n"));
            cpp.push_str(&format!("  uint64_t total = 0; uint64_t hit = 0;\n"));
            cpp.push_str(&format!("  for (uint32_t i = 0; i < {n_cov}; i++) {{ total++; if ({class}::_arch_cov[i]) hit++; }}\n"));
            cpp.push_str(&format!("  fprintf(stderr, \"[{class}] FSM coverage: %llu/%llu hit (%.1f%%)\\n\", (unsigned long long)hit, (unsigned long long)total, total ? (100.0 * hit / total) : 0.0);\n"));
            // --coverage-dat: also append per-point Verilator-compatible
            // lines for the FSM coverage points.
            if let Some(path) = &self.coverage_dat {
                let path_lit = path.replace('\\', "\\\\").replace('"', "\\\"");
                cpp.push_str(&format!("  FILE* _dat = _arch_cov_dat_open(\"{path_lit}\");\n"));
            }
            for (i, p) in cov_reg.borrow().points.iter().enumerate() {
                let (file_disp, line_no) = if let Some(sm) = &self.source_map {
                    sm.locate(p.span_start)
                        .map(|(f, l)| (f.to_string(), l))
                        .unwrap_or_else(|| (String::new(), 0))
                } else {
                    (String::new(), 0)
                };
                let location = if !file_disp.is_empty() {
                    format!("{file_disp}:{line_no}")
                } else {
                    format!("point[{i}]")
                };
                let label_escaped = p.label.replace('"', "\\\"");
                cpp.push_str(&format!(
                    "  fprintf(stderr, \"  {location} ({}) [{label_escaped}]: %llu hits%s\\n\", (unsigned long long){class}::_arch_cov[{i}], {class}::_arch_cov[{i}] ? \"\" : \" *NOT HIT*\");\n",
                    p.kind
                ));
                if self.coverage_dat.is_some() && !file_disp.is_empty() {
                    let file_esc = file_disp.replace('\\', "\\\\").replace('"', "\\\"");
                    let page = match p.kind {
                        "state" | "trans" => "v_user/fsm",
                        _                 => "v_user",
                    };
                    cpp.push_str(&format!(
                        "  if (_dat) fprintf(_dat, \"C '\" \"\\x01\" \"file\" \"\\x02\" \"{file_esc}\" \"\\x01\" \"line\" \"\\x02\" \"{line_no}\" \"\\x01\" \"page\" \"\\x02\" \"{page}\" \"\\x01\" \"comment\" \"\\x02\" \"{kind} {comment}\" \"' %llu\\n\", (unsigned long long){class}::_arch_cov[{i}]);\n",
                        kind = p.kind, comment = label_escaped
                    ));
                }
            }
            if self.coverage_dat.is_some() {
                cpp.push_str("  if (_dat) fclose(_dat);\n");
            }
            cpp.push_str("}\n");
            cpp.push_str("struct _ArchCovInit { _ArchCovInit() { atexit(_arch_cov_dump); } };\n");
            cpp.push_str("static _ArchCovInit _arch_cov_init;\n");
            cpp.push_str("} // namespace\n\n");
        }

        h.push_str("};\n");

        SimModel { class_name: class, header: h, impl_: cpp }
    }
}
