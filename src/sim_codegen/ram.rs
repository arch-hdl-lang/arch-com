//! `gen_ram` emitter — extracted from `sim_codegen/mod.rs` to keep the
//! growing file reviewable. Mirrors the pattern from PR #73 (fsm,
//! pipeline). Lives in a submodule so `super::` keeps visibility of the
//! shared free-function helpers.

use std::collections::HashMap;

use super::{SimCodegen, SimModel};
use super::*;

impl<'a> SimCodegen<'a> {
    pub(crate) fn gen_ram(&self, r: &RamDecl) -> SimModel {
        let name = &r.name.name;
        let class = format!("V{name}");

        // Extract DEPTH param
        let depth: u64 = r.params.iter()
            .find(|p| p.name.name == "DEPTH")
            .and_then(|p| p.default.as_ref())
            .map(|e| match &e.kind {
                ExprKind::Literal(LitKind::Dec(v)) => *v,
                _ => 256,
            })
            .unwrap_or(256);

        // Build type-param map: param name → resolved TypeExpr
        // e.g. `param T: type = UInt<54>` → "T" → UInt<54>
        let type_params: HashMap<String, &TypeExpr> = r.params.iter()
            .filter_map(|p| {
                if let ParamKind::Type(ty) = &p.kind {
                    Some((p.name.name.clone(), ty))
                } else {
                    None
                }
            })
            .collect();

        // Resolve a signal TypeExpr width through the param map
        let resolve_sig_bits = |ty: &TypeExpr| -> u32 {
            let resolved = match ty {
                TypeExpr::Named(n) => type_params.get(&n.name).copied().unwrap_or(ty),
                other => other,
            };
            type_width(resolved)
        };

        // Extract data width from output port signal type
        let data_bits: u32 = r.port_groups.iter()
            .flat_map(|pg| pg.signals.iter())
            .find(|s| s.direction == Direction::Out)
            .map(|s| resolve_sig_bits(&s.ty))
            .unwrap_or(32);

        let elem_ty = if data_bits > 128 { format!("VlWide<{}>", wide_words(data_bits)) }
                      else if data_bits > 64 { "_arch_u128".to_string() }
                      else { cpp_uint(data_bits).to_string() };
        let port_elem_ty = if data_bits > 64 { format!("VlWide<{}>", wide_words(data_bits)) } else { cpp_uint(data_bits).to_string() };
        let is_wide = data_bits > 64;

        // Flatten port groups into (full_name, direction)
        struct FlatSig { full_name: String, dir: Direction }
        let mut flat_sigs: Vec<FlatSig> = Vec::new();
        for pg in &r.port_groups {
            for sig in &pg.signals {
                flat_sigs.push(FlatSig {
                    full_name: format!("{}_{}", pg.name.name, sig.name.name),
                    dir: sig.direction,
                });
            }
        }
        let out_sigs: Vec<&FlatSig> = flat_sigs.iter().filter(|s| s.dir == Direction::Out).collect();

        // ── Header ──
        let mut h = String::new();
        h.push_str("#pragma once\n#include <cstdint>\n#include <cstring>\n#include <cstdio>\n#include <cstdlib>\n#include \"verilated.h\"\n\n");
        h.push_str(&format!("class {class} {{\npublic:\n"));
        h.push_str("  uint8_t clk;\n");

        for fs in &flat_sigs {
            let ty_str: String = if fs.dir == Direction::Out {
                port_elem_ty.clone()
            } else {
                let orig_ty = r.port_groups.iter()
                    .flat_map(|pg| pg.signals.iter().map(move |s| (format!("{}_{}", pg.name.name, s.name.name), &s.ty)))
                    .find(|(n, _)| *n == fs.full_name)
                    .map(|(_, ty)| ty);
                match orig_ty {
                    Some(TypeExpr::Bool) => "uint8_t".to_string(),
                    Some(ty) => {
                        let b = resolve_sig_bits(ty);
                        if b > 64 { port_elem_ty.clone() } else { cpp_uint(b).to_string() }
                    }
                    None => "uint32_t".to_string(),
                }
            };
            h.push_str(&format!("  {} {};\n", ty_str, fs.full_name));
        }

        h.push('\n');
        h.push_str(&format!("  {class}() : clk(0)"));
        for fs in &flat_sigs {
            if is_wide && fs.dir == Direction::Out { /* VlWide memset below */ } else {
                h.push_str(&format!(", {}(0)", fs.full_name));
            }
        }
        h.push_str(", _clk_prev(0) {\n");
        // --check-uninit-ram: track per-cell valid bits for non-ROM RAMs
        let uninit_ram_check = self.check_uninit_ram && !matches!(r.kind, RamKind::Rom);
        // Initialize memory
        match &r.init {
            Some(RamInit::Array(values)) => {
                h.push_str("    memset(_mem, 0, sizeof(_mem));\n");
                if uninit_ram_check {
                    h.push_str("    memset(_mem_valid, 0, sizeof(_mem_valid));\n");
                }
                for (i, v) in values.iter().enumerate() {
                    h.push_str(&format!("    _mem[{i}] = 0x{v:X}ULL;\n"));
                    if uninit_ram_check {
                        h.push_str(&format!("    _mem_valid[{i}] = true;\n"));
                    }
                }
            }
            Some(RamInit::File(path, format)) => {
                // Load hex or bin file at construction
                h.push_str("    memset(_mem, 0, sizeof(_mem));\n");
                if uninit_ram_check {
                    h.push_str("    memset(_mem_valid, 0, sizeof(_mem_valid));\n");
                }
                h.push_str("    { FILE* _f = fopen(\"");
                h.push_str(path);
                h.push_str("\", \"r\");\n");
                h.push_str("      if (_f) {\n");
                h.push_str(&format!("        char _line[256]; int _i = 0;\n"));
                h.push_str("        while (fgets(_line, sizeof(_line), _f) && _i < ");
                h.push_str(&format!("{depth}"));
                h.push_str(") {\n");
                match format {
                    FileFormat::Hex => h.push_str("          _mem[_i] = strtoull(_line, NULL, 16);\n"),
                    FileFormat::Bin => h.push_str("          _mem[_i] = strtoull(_line, NULL, 2);\n"),
                }
                if uninit_ram_check {
                    h.push_str("          _mem_valid[_i] = true;\n");
                }
                h.push_str("          _i++;\n");
                h.push_str("        }\n");
                h.push_str("        fclose(_f);\n");
                h.push_str("      }\n");
                h.push_str("    }\n");
            }
            _ => {
                h.push_str("    memset(_mem, 0, sizeof(_mem));\n");
                if uninit_ram_check {
                    h.push_str("    memset(_mem_valid, 0, sizeof(_mem_valid));\n");
                }
            }
        }
        for fs in &out_sigs {
            if is_wide {
                h.push_str(&format!("    memset(&{}, 0, sizeof({}));\n", fs.full_name, fs.full_name));
            }
        }
        h.push_str("  }\n");
        h.push_str(&format!("  explicit {class}(VerilatedContext*) : {class}() {{}}\n"));
        h.push_str("  void eval();\n  void eval_posedge();\n  void eval_comb();\n  void final() { trace_close(); }\n");
        h.push_str("private:\n");
        h.push_str("  uint8_t _clk_prev;\n");
        h.push_str(&format!("  {} _mem[{}];\n", elem_ty, depth));
        if uninit_ram_check {
            h.push_str(&format!("  bool _mem_valid[{}];\n", depth));
        }
        for fs in &out_sigs {
            h.push_str(&format!("  {} _r_{};\n", elem_ty, fs.full_name));
            if r.latency == 2 {
                h.push_str(&format!("  {} _r2_{};\n", elem_ty, fs.full_name));
            }
        }

        // ── Implementation ──
        let mut cpp = String::new();
        cpp.push_str(&format!("#include \"{class}.h\"\n\n"));

        // --check-uninit-ram: helpers to mark writes valid and warn on uninit reads.
        // Read check uses a static guard so each RAM warns at most once per run.
        let write_mark = |addr: &str| -> String {
            if uninit_ram_check { format!("    _mem_valid[{addr}] = true;\n") } else { String::new() }
        };
        let read_check = |addr: &str, indent: &str| -> String {
            if !uninit_ram_check { return String::new(); }
            format!(
                "{indent}if (!_mem_valid[{addr}]) {{ static bool _w = false; if (!_w) {{ fprintf(stderr, \"WARNING: read of uninitialized RAM cell '{name}[%d]' (no prior write, no init)\\n\", (int)({addr})); _w = true; }} }}\n"
            )
        };

        cpp.push_str(&format!("void {class}::eval() {{\n"));
        cpp.push_str("  if (!_trace_fp && Verilated::traceFile() && Verilated::claimTrace())\n");
        cpp.push_str("    trace_open(Verilated::traceFile());\n");
        cpp.push_str("  eval_posedge();\n");
        cpp.push_str("  eval_comb();\n");
        cpp.push_str("  if (_trace_fp) trace_dump(_trace_time++);\n");
        cpp.push_str("}\n\n");

        cpp.push_str(&format!("void {class}::eval_posedge() {{\n"));
        cpp.push_str("  bool _rising = (clk && !_clk_prev);\n");
        cpp.push_str("  _clk_prev = clk;\n");
        cpp.push_str("  if (!_rising) return;\n");
        match r.kind {
            RamKind::Single => {
                let pg = &r.port_groups[0];
                let pfx = &pg.name.name;
                let has_wen = pg.signals.iter().any(|s| s.name.name == "wen");
                let wdata_name = pg.signals.iter()
                    .find(|s| s.direction == Direction::In && (s.name.name == "wdata" || s.name.name == "data"))
                    .map(|s| format!("{pfx}_{}", s.name.name))
                    .unwrap_or_else(|| format!("{pfx}_wdata"));
                let out_name = out_sigs.first().map(|s| s.full_name.as_str()).unwrap_or("rdata");
                let addr_expr = format!("{pfx}_addr");

                cpp.push_str(&format!("  if ({pfx}_en) {{\n"));
                if has_wen {
                    cpp.push_str(&format!("    if ({pfx}_wen) {{ _mem[{pfx}_addr] = {wdata_name};{mark} }}\n",
                        mark = if uninit_ram_check { format!(" _mem_valid[{pfx}_addr] = true;") } else { String::new() }));
                    match r.latency {
                        1 | 2 => {
                            cpp.push_str(&format!("    if (!{pfx}_wen) {{\n"));
                            cpp.push_str(&read_check(&addr_expr, "      "));
                            cpp.push_str(&format!("      _r_{out_name} = _mem[{pfx}_addr];\n"));
                            cpp.push_str("    }\n");
                        }
                        0 | _ => {}
                    }
                } else {
                    cpp.push_str(&format!("    _mem[{pfx}_addr] = {wdata_name};\n"));
                    cpp.push_str(&write_mark(&addr_expr));
                }
                cpp.push_str("  }\n");
                if r.latency == 2 {
                    cpp.push_str(&format!("  _r2_{out_name} = _r_{out_name};\n"));
                }
            }
            RamKind::SimpleDual => {
                let wr_pg = r.port_groups.iter().find(|pg|
                    pg.signals.iter().any(|s| s.direction == Direction::In && (s.name.name == "data" || s.name.name == "wdata"))
                ).unwrap_or(&r.port_groups[1]);
                let rd_pg = r.port_groups.iter().find(|pg|
                    pg.signals.iter().any(|s| s.direction == Direction::Out)
                ).unwrap_or(&r.port_groups[0]);

                let wpfx = &wr_pg.name.name;
                let rpfx = &rd_pg.name.name;
                let w_data_name = wr_pg.signals.iter()
                    .find(|s| s.direction == Direction::In && (s.name.name == "data" || s.name.name == "wdata"))
                    .map(|s| format!("{wpfx}_{}", s.name.name))
                    .unwrap_or_else(|| format!("{wpfx}_data"));
                let out_name = out_sigs.first().map(|s| s.full_name.as_str()).unwrap_or("rd_port_data");
                let wr_addr = format!("{wpfx}_addr");
                let rd_addr = format!("{rpfx}_addr");

                // Write path
                cpp.push_str(&format!("  if ({wpfx}_en) {{\n"));
                if is_wide {
                    cpp.push_str(&format!("    memcpy(&_mem[{wpfx}_addr], &{w_data_name}, sizeof({elem_ty}));\n"));
                } else {
                    cpp.push_str(&format!("    _mem[{wpfx}_addr] = {w_data_name};\n"));
                }
                cpp.push_str(&write_mark(&wr_addr));
                cpp.push_str("  }\n");
                // Read path
                match r.latency {
                    1 | 2 => {
                        cpp.push_str(&format!("  if ({rpfx}_en) {{\n"));
                        cpp.push_str(&read_check(&rd_addr, "    "));
                        if is_wide {
                            cpp.push_str(&format!("    memcpy(&_r_{out_name}, &_mem[{rpfx}_addr], sizeof({elem_ty}));\n"));
                        } else {
                            cpp.push_str(&format!("    _r_{out_name} = _mem[{rpfx}_addr];\n"));
                        }
                        cpp.push_str("  }\n");
                    }
                    0 | _ => {}
                }
                if r.latency == 2 {
                    if is_wide {
                        cpp.push_str(&format!("  memcpy(&_r2_{out_name}, &_r_{out_name}, sizeof({elem_ty}));\n"));
                    } else {
                        cpp.push_str(&format!("  _r2_{out_name} = _r_{out_name};\n"));
                    }
                }
            }
            RamKind::TrueDual => {
                cpp.push_str("  // TrueDual: not yet implemented\n");
            }
            RamKind::Rom => {
                // ROM: read-only, no writes in posedge
                // For latency >= 1, latch the read output
                if r.latency >= 1 {
                    let pg = &r.port_groups[0];
                    let rpfx = &pg.name.name;
                    let out_name = out_sigs.first().map(|s| s.full_name.as_str()).unwrap_or("data");
                    let has_en = pg.signals.iter().any(|s| s.name.name == "en");
                    if has_en {
                        cpp.push_str(&format!("  if ({rpfx}_en) _r_{out_name} = _mem[{rpfx}_addr];\n"));
                    } else {
                        cpp.push_str(&format!("  _r_{out_name} = _mem[{rpfx}_addr];\n"));
                    }
                    if r.latency == 2 {
                        cpp.push_str(&format!("  _r2_{out_name} = _r_{out_name};\n"));
                    }
                }
            }
        }
        cpp.push_str("}\n\n");

        cpp.push_str(&format!("void {class}::eval_comb() {{\n"));
        for fs in &out_sigs {
            match r.latency {
                0 => {
                    let rpfx = r.port_groups.iter()
                        .find(|pg| pg.signals.iter().any(|s| s.direction == Direction::Out))
                        .map(|pg| pg.name.name.as_str())
                        .unwrap_or("access");
                    let rd_addr = format!("{rpfx}_addr");
                    cpp.push_str(&read_check(&rd_addr, "  "));
                    if is_wide {
                        cpp.push_str(&format!("  memcpy(&{}, &_mem[{rpfx}_addr], sizeof({}));\n", fs.full_name, fs.full_name));
                    } else {
                        cpp.push_str(&format!("  {} = _mem[{rpfx}_addr];\n", fs.full_name));
                    }
                }
                1 => {
                    if is_wide {
                        cpp.push_str(&format!("  memcpy(&{}, &_r_{}, sizeof({}));\n", fs.full_name, fs.full_name, fs.full_name));
                    } else {
                        cpp.push_str(&format!("  {} = _r_{};\n", fs.full_name, fs.full_name));
                    }
                }
                2 => {
                    if is_wide {
                        cpp.push_str(&format!("  memcpy(&{}, &_r2_{}, sizeof({}));\n", fs.full_name, fs.full_name, fs.full_name));
                    } else {
                        cpp.push_str(&format!("  {} = _r2_{};\n", fs.full_name, fs.full_name));
                    }
                }
                _ => {}
            }
        }
        cpp.push_str("}\n");

        let extra_sigs: Vec<(&str, &str, u32)> = vec![];
        add_trace_to_simple_construct(&mut h, &mut cpp, &class, name, &r.ports, &extra_sigs);
        h.push_str("};\n");

        SimModel { class_name: class, header: h, impl_: cpp }
    }
}
