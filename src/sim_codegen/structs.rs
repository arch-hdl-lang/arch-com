//! `structs` — extracted from `sim_codegen/mod.rs` (P4 phase 1
//! move-only split, following the pattern established by fsm.rs/pipeline.rs/
//! ram.rs/etc). No logic changes; `use super::*` keeps visibility of the
//! shared free-function helpers and the `Ctx`/`SimCodegen` types.

use super::*;

impl<'a> SimCodegen<'a> {
    /// Generate VStructs.h containing C++ type definitions for all ARCH structs and enums.
    /// Collects from both file-scope and inside `package` declarations.
    pub(super) fn gen_structs_file(&self) -> SimModel {
        let mut h = String::new();
        h.push_str(
            "#pragma once\n#include <cstdint>\n#include <cstring>\n#include <array>\n#include \"verilated.h\"\n\n",
        );

        // Gather all enums and structs, whether declared at file scope or inside packages.
        let mut enums: Vec<&EnumDecl> = Vec::new();
        let mut structs: Vec<&StructDecl> = Vec::new();
        for item in &self.source.items {
            match item {
                Item::Enum(e) => enums.push(e),
                Item::Struct(s) => structs.push(s),
                Item::Package(p) => {
                    for e in &p.enums {
                        enums.push(e);
                    }
                    for s in &p.structs {
                        structs.push(s);
                    }
                }
                _ => {}
            }
        }

        for e in &enums {
            // Enums are uint32_t aliases — variants are used as integer indices
            h.push_str(&format!("typedef uint32_t {};\n", e.name.name));
            for (i, v) in e.variants.iter().enumerate() {
                h.push_str(&format!(
                    "static const uint32_t {}_{} = {}u;\n",
                    e.name.name, v.name, i
                ));
            }
            h.push('\n');
        }

        // C++ struct emission for ARCH packed structs.
        //
        // Canonical ARCH bit layout is first-declared-field = MSB, last-declared = LSB
        // (SV convention, see codegen.rs::emit_struct and the Language Specification
        // §"Packed bit layout"). The C++ struct below lays fields out in declaration
        // order in memory — this is the natural C++ idiom and what pybind11 expects
        // when we expose per-field handles via `.def_readwrite`. Per-field access is
        // bit-order-agnostic, so the C++ memory layout and the SV bit layout don't
        // need to agree structurally.
        //
        // ⚠ Future maintainers: on a little-endian host (x86_64, ARM64 in default
        // mode) a `memcpy`/`reinterpret_cast` of this C++ struct into a wide integer
        // puts the FIRST field at the LSBs — the OPPOSITE of ARCH's canonical bit
        // layout. If you add a code path that serializes a whole struct to a single
        // integer (a `struct as UInt<N>` codegen, a pybind11 `__int__` / `.value`
        // shim, a VCD compound-signal trace, etc.), you MUST explicitly concatenate
        // `first_field → MSB, last_field → LSB` — do NOT rely on `memcpy` or
        // `reinterpret_cast`.
        for s in &structs {
            h.push_str(&format!("struct {} {{\n", s.name.name));
            for f in &s.fields {
                h.push_str(&format!(
                    "  {};\n",
                    cpp_field_decl(&f.name.name, &f.ty, &[])
                ));
            }
            h.push_str(&format!(
                "  {}() {{ std::memset(this, 0, sizeof(*this)); }}\n",
                s.name.name
            ));
            h.push_str(&format!(
                "  explicit {}(uint64_t v) {{ (void)v; std::memset(this, 0, sizeof(*this)); }}\n",
                s.name.name
            ));
            h.push_str(&format!("  {}& operator=(uint64_t v) {{ (void)v; std::memset(this, 0, sizeof(*this)); return *this; }}\n", s.name.name));
            h.push_str("};\n\n");
        }

        // Bus-as-wire support: emit a plain C++ struct for every `bus`, with
        // one field per effective (flattened) signal. Direction information is
        // intentionally dropped — when a bus appears as a `wire` (not a port),
        // each signal is just a named piece of data driven by whichever
        // module's assignment reaches it. Field directions only matter at
        // port boundaries, where the perspective (initiator/target) chooses
        // which side drives which field.
        // Collect buses from both file scope AND packages. `bus` in a package
        // is equivalent to file-scope `bus` — just grouped with the types
        // that define the same package's interface.
        let mut buses: Vec<&BusDecl> = Vec::new();
        for item in &self.source.items {
            match item {
                Item::Bus(b) => buses.push(b),
                Item::Package(p) => {
                    for b in &p.buses {
                        buses.push(b);
                    }
                }
                _ => {}
            }
        }
        for b in &buses {
            // Seed with each bus param's declared default so `generate_if`
            // gates (e.g. `generate_if READ` / `WRITE`) evaluate as the
            // bus author intended for the "no overrides" struct. Without
            // this, the param_map is empty and every conditional branch
            // folds to false, producing an empty struct that breaks any
            // sim consumer that touches a bus field.
            let param_map: HashMap<String, &Expr> = b
                .params
                .iter()
                .filter_map(|p| p.default.as_ref().map(|d| (p.name.name.clone(), d)))
                .collect();
            let effective = crate::resolve::BusInfo {
                name: b.name.name.clone(),
                params: b.params.clone(),
                signals: b
                    .signals
                    .iter()
                    .map(|p| (p.name.name.clone(), p.direction, p.ty.clone()))
                    .collect(),
                generates: b.generates.clone(),
                handshakes: b.handshakes.clone(),
                credit_channels: b.credit_channels.clone(),
                tlm_methods: b.tlm_methods.clone(),
            }
            .effective_signals(&param_map);
            h.push_str(&format!("struct {} {{\n", b.name.name));
            let mut field_inits = Vec::new();
            let mut ctor_body = Vec::new();
            for (sname, _dir, sty) in &effective {
                if vec_array_info_with_params(sty, &b.params).is_some() {
                    h.push_str(&format!("  {};\n", cpp_field_decl(sname, sty, &[])));
                    // `sname` is now a `std::array<...>` (see cpp_field_decl /
                    // cpp_std_array_type), not a raw C array — it no longer
                    // decays to a pointer, so `memset` needs `.data()` to
                    // reach the underlying contiguous storage. Byte-identical
                    // zero-fill to the old raw-array behavior.
                    ctor_body.push(format!(
                        "std::memset({}.data(), 0, sizeof({}));",
                        sname, sname
                    ));
                } else {
                    let ty = cpp_internal_type_with_params(sty, &b.params);
                    h.push_str(&format!("  {} {};\n", ty, sname));
                    if matches!(sty, TypeExpr::Named(_)) {
                        field_inits.push(format!("{}()", sname));
                    } else {
                        field_inits.push(format!("{}(0)", sname));
                    }
                }
            }
            if field_inits.is_empty() && ctor_body.is_empty() {
                h.push_str(&format!("  {}() {{}}\n", b.name.name));
            } else if field_inits.is_empty() {
                h.push_str(&format!(
                    "  {}() {{ {} }}\n",
                    b.name.name,
                    ctor_body.join(" ")
                ));
            } else if ctor_body.is_empty() {
                h.push_str(&format!(
                    "  {}() : {} {{}}\n",
                    b.name.name,
                    field_inits.join(", ")
                ));
            } else {
                h.push_str(&format!(
                    "  {}() : {} {{ {} }}\n",
                    b.name.name,
                    field_inits.join(", "),
                    ctor_body.join(" ")
                ));
            }
            h.push_str("};\n\n");
        }

        SimModel {
            class_name: "VStructs".to_string(),
            header: h,
            impl_: "#include \"VStructs.h\"\n".to_string(),
        }
    }
}
