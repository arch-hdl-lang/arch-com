//! `expr_codegen` — extracted from `sim_codegen/mod.rs` (P4 phase 1
//! move-only split, following the pattern established by fsm.rs/pipeline.rs/
//! ram.rs/etc). No logic changes; `use super::*` keeps visibility of the
//! shared free-function helpers and the `Ctx`/`SimCodegen` types.

use super::*;

/// Cast expression to `bits`-wide C++ type.
pub(super) fn cast_to_bits(expr: &str, bits: u32) -> String {
    // Must mask to the exact bit-width, since C++ types are wider than the
    // HDL type (e.g. UInt<2> stored in uint8_t).
    if bits >= 64 {
        // 64-bit or wider: cast is sufficient (or use u128 path)
        format!("({})({})", cpp_uint(bits), expr)
    } else {
        let mask = (1u64 << bits) - 1;
        format!("({})((({}) & 0x{:X}ULL))", cpp_uint(bits), expr, mask)
    }
}

/// Cast expression to a signed HDL scalar width and sign-extend into the
/// selected C++ signed storage type. For example, SInt<40> uses int64_t
/// storage but bit 39 is the HDL sign bit, so the 40-bit truncated pattern
/// must be shifted through bit 63 before arithmetic use.
pub(super) fn cast_to_signed_bits(expr: &str, bits: u32) -> String {
    if bits >= 64 {
        format!("({})({})", cpp_sint(bits), expr)
    } else {
        let mask = (1u64 << bits) - 1;
        let cpp_bits = if bits <= 8 {
            8
        } else if bits <= 16 {
            16
        } else if bits <= 32 {
            32
        } else {
            64
        };
        let shift = cpp_bits - bits;
        let ty = cpp_sint(bits);
        format!("(({ty})(((uint64_t)({expr}) & 0x{mask:X}ULL) << {shift}) >> {shift})")
    }
}

/// Bit-range extraction from a narrow value: `(expr >> lo) & mask`.
pub(super) fn bit_range(expr: &str, hi: u32, lo: u32) -> String {
    let width = hi - lo + 1;
    let mask = if width >= 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    };
    format!("(({} >> {}) & 0x{:X}ULL)", expr, lo, mask)
}

/// Bit-range extraction from a `_arch_u128` value.
pub(super) fn bit_range_u128(expr: &str, hi: u32, lo: u32) -> String {
    let width = hi - lo + 1;
    let result_type = cpp_uint(width);
    if lo == 0 && width >= 128 {
        format!("({result_type})({})", expr)
    } else if lo == 0 {
        let mask = if width >= 64 {
            u64::MAX
        } else {
            (1u64 << width) - 1
        };
        format!(
            "({result_type})(((_arch_u128)({}) & (_arch_u128)0x{:X}ULL))",
            expr, mask
        )
    } else {
        let mask = if width >= 64 {
            u64::MAX
        } else {
            (1u64 << width) - 1
        };
        format!(
            "({result_type})(((_arch_u128)({}) >> {}) & (_arch_u128)0x{:X}ULL)",
            expr, lo, mask
        )
    }
}

/// Convert SV/ARCH format string tokens to printf equivalents.
pub(super) fn sv_fmt_to_printf(s: &str) -> String {
    s.replace("%0t", "%lu")
        .replace("%0d", "%lld")
        .replace("%0h", "%llx")
        .replace("%0b", "%llu")
        .replace("%t", "%lu")
        .replace("%h", "%llx")
        .replace("%d", "%lld")
        .replace("%b", "%llu")
}

pub(super) struct Ctx<'a> {
    pub(super) reg_names: &'a HashSet<String>,
    pub(super) port_names: &'a HashSet<String>,
    pub(super) let_names: &'a HashSet<String>,
    /// Map of module-scope let-binding names → their RHS expressions.
    /// Populated via `Ctx::with_let_values`. Used by `Stmt::Match` to
    /// fold `Pattern::Ident` arms into literal case labels.
    pub(super) let_values: Option<&'a HashMap<String, Expr>>,
    pub(super) inst_names: &'a HashSet<String>,
    /// Signals whose type is >64 bits wide (require special handling).
    pub(super) wide_names: &'a HashSet<String>,
    /// Signal name → bit width for known signals (used for concat width inference).
    pub(super) widths: &'a HashMap<String, u32>,
    /// Signal names whose HDL scalar type is signed.
    pub(super) signed_names: &'a HashSet<String>,
    /// Signal name → floating-point format (FP32/BF16). Used to dispatch
    /// `+ - *` and comparisons to the `_arch_fp.h` helpers instead of integer
    /// operators on the bit-pattern carrier.
    pub(super) float_names: &'a HashMap<String, FpFmt>,
    pub(super) posedge_lhs: bool,
    /// FSM mode: regs are public members, no `_` prefix on reads
    pub(super) fsm_mode: bool,
    pub(super) enum_map: &'a HashMap<String, Vec<(String, u64)>>,
    /// Bus port names (for FieldAccess rewriting: itcm.cmd_valid → itcm_cmd_valid).
    pub(super) bus_ports: &'a HashSet<String>,
    /// Reset port name → level, for `.asserted` polarity abstraction.
    pub(super) reset_levels: &'a HashMap<String, ResetLevel>,
    /// Reg/wire names whose type is Vec<T,N> — these use C array subscript `[i]`.
    /// All other subscripts on scalar UInt/SInt use bit extraction `(x >> i) & 1`.
    pub(super) vec_names: Option<&'a HashSet<String>>,
    /// Names of *2D* Vec<Vec<_,_>,_> wires/regs (today: Vec-of-Vec-of-bus
    /// `wire edges: Vec<Vec<B, N>, M>`). When the outer Index returns
    /// `_let_edges[m]`, the result is still a Vec — the inner subscript
    /// must keep using C array indexing `[n]`, NOT fall into the bit-shift
    /// path for scalar types.
    pub(super) vec_2d_names: Option<&'a HashSet<String>>,
    /// Vec<T,N> sizes by name (element count). Used for runtime bounds-check codegen.
    pub(super) vec_sizes: Option<&'a HashMap<String, u64>>,
    /// FSM Vec port-regs: always resolve to `_name` (internal C array), regardless of fsm_mode.
    /// These ports have flat public fields (name_0..name_N-1) but internal storage `_name[N]`.
    pub(super) fsm_vec_port_regs: Option<&'a HashSet<String>>,
    /// Identifier substitutions active while emitting a Vec method predicate
    /// (e.g. "item" → "vec[3]", "index" → "3"). Checked first in the Ident
    /// branch of `cpp_expr`; None or missing key means normal resolution.
    pub(super) ident_subst: Option<&'a HashMap<String, String>>,
    /// Loop-variable → integer-value substitutions pushed during static
    /// unrolling of `for` loops over Vec-of-bus indexed access. RefCell
    /// for interior mutability — the for-loop emitter mutates per
    /// iteration while emit_stmt walks the body. None = no active unroll.
    pub(super) loop_var_subst: Option<&'a std::cell::RefCell<HashMap<String, u32>>>,
    /// Vec-of-bus port name → element count. Used by the for-loop emitter
    /// to decide whether the body needs static unrolling.
    pub(super) vec_of_bus_port_count: Option<&'a HashMap<String, u32>>,
    /// Same as `vec_of_bus_port_count` but for `wire w: Vec<BusName, N>;`.
    pub(super) vec_of_bus_wire_count: Option<&'a HashMap<String, u32>>,
    /// Branch-coverage registry for the current module. None when --coverage
    /// is off; Some(_) when on. emit_*_if_else allocates counter ids here
    /// and emits `_arch_cov[N]++;` at the start of each arm.
    pub(super) coverage: Option<&'a std::cell::RefCell<CoverageRegistry>>,
    /// Module params (regular + local) for param-aware constant folding in
    /// width-bearing positions. Used by `eval_width_in` to fold expressions
    /// like `CounterWidth-1` in `BitSlice` hi/lo and `PartSelect` width.
    /// Empty by default; populated via [`Ctx::with_params`] at module entry.
    pub(super) params: &'a [ParamDecl],
    /// Reg names whose shadow valid bit should be marked true when a seq
    /// assignment writes the reg.
    pub(super) vinit_regs: Option<&'a HashSet<String>>,
    /// Declared TypeExprs of scope signals (ports/regs/wires/typed lets).
    /// Basis for composite float resolution: `Vec<FP32,N>[i]` and
    /// `s.field` reads take their element/field float format from here.
    pub(super) decl_types: Option<&'a HashMap<String, TypeExpr>>,
    /// Struct name -> field list, for FieldAccess float resolution.
    pub(super) struct_defs: Option<&'a HashMap<String, Vec<(String, TypeExpr)>>>,
}

impl<'a> Ctx<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        reg_names: &'a HashSet<String>,
        port_names: &'a HashSet<String>,
        let_names: &'a HashSet<String>,
        inst_names: &'a HashSet<String>,
        wide_names: &'a HashSet<String>,
        widths: &'a HashMap<String, u32>,
        enum_map: &'a HashMap<String, Vec<(String, u64)>>,
        bus_ports: &'a HashSet<String>,
    ) -> Self {
        static EMPTY_RESET_LEVELS: std::sync::OnceLock<HashMap<String, ResetLevel>> =
            std::sync::OnceLock::new();
        static EMPTY_SIGNED_NAMES: std::sync::OnceLock<HashSet<String>> =
            std::sync::OnceLock::new();
        static EMPTY_FLOAT_NAMES: std::sync::OnceLock<HashMap<String, FpFmt>> =
            std::sync::OnceLock::new();
        let reset_levels = EMPTY_RESET_LEVELS.get_or_init(HashMap::new);
        let signed_names = EMPTY_SIGNED_NAMES.get_or_init(HashSet::new);
        let float_names = EMPTY_FLOAT_NAMES.get_or_init(HashMap::new);
        static EMPTY_PARAMS: &[ParamDecl] = &[];
        Ctx {
            reg_names,
            port_names,
            let_names,
            let_values: None,
            inst_names,
            wide_names,
            widths,
            signed_names,
            float_names,
            posedge_lhs: false,
            fsm_mode: false,
            enum_map,
            bus_ports,
            reset_levels,
            vec_names: None,
            vec_2d_names: None,
            vec_sizes: None,
            fsm_vec_port_regs: None,
            ident_subst: None,
            loop_var_subst: None,
            vec_of_bus_port_count: None,
            vec_of_bus_wire_count: None,
            coverage: None,
            params: EMPTY_PARAMS,
            vinit_regs: None,
            decl_types: None,
            struct_defs: None,
        }
    }

    pub(super) fn with_vec_of_bus(
        mut self,
        ports: &'a HashMap<String, u32>,
        wires: &'a HashMap<String, u32>,
        subst: &'a std::cell::RefCell<HashMap<String, u32>>,
    ) -> Self {
        self.vec_of_bus_port_count = Some(ports);
        self.vec_of_bus_wire_count = Some(wires);
        self.loop_var_subst = Some(subst);
        self
    }

    pub(super) fn with_signed_names(mut self, signed_names: &'a HashSet<String>) -> Self {
        self.signed_names = signed_names;
        self
    }

    pub(super) fn with_float_names(mut self, float_names: &'a HashMap<String, FpFmt>) -> Self {
        self.float_names = float_names;
        self
    }

    pub(super) fn with_params(mut self, params: &'a [ParamDecl]) -> Self {
        self.params = params;
        self
    }

    pub(super) fn with_vinit_regs(mut self, vinit_regs: &'a HashSet<String>) -> Self {
        self.vinit_regs = Some(vinit_regs);
        self
    }

    pub(super) fn with_let_values(mut self, let_values: &'a HashMap<String, Expr>) -> Self {
        self.let_values = Some(let_values);
        self
    }

    pub(super) fn with_vec_sizes(mut self, vec_sizes: &'a HashMap<String, u64>) -> Self {
        self.vec_sizes = Some(vec_sizes);
        self
    }

    pub(super) fn with_reset_levels(
        mut self,
        reset_levels: &'a HashMap<String, ResetLevel>,
    ) -> Self {
        self.reset_levels = reset_levels;
        self
    }

    pub(super) fn with_vec_names(mut self, vec_names: &'a HashSet<String>) -> Self {
        self.vec_names = Some(vec_names);
        self
    }

    pub(super) fn with_vec_2d_names(mut self, vec_2d_names: &'a HashSet<String>) -> Self {
        self.vec_2d_names = Some(vec_2d_names);
        self
    }

    pub(super) fn with_fsm_vec_port_regs(mut self, fsm_vec_port_regs: &'a HashSet<String>) -> Self {
        self.fsm_vec_port_regs = Some(fsm_vec_port_regs);
        self
    }

    pub(super) fn with_ident_subst(mut self, ident_subst: &'a HashMap<String, String>) -> Self {
        self.ident_subst = Some(ident_subst);
        self
    }

    pub(super) fn with_coverage(
        mut self,
        reg: Option<&'a std::cell::RefCell<CoverageRegistry>>,
    ) -> Self {
        self.coverage = reg;
        self
    }

    pub(super) fn posedge(mut self) -> Self {
        self.posedge_lhs = true;
        self
    }

    /// Resolve a name to its C++ field/variable name.
    pub(super) fn resolve_name(&self, name: &str, is_lhs: bool) -> String {
        // FSM Vec port-regs always use `_name` (internal C array) regardless of mode.
        if self.fsm_vec_port_regs.map_or(false, |s| s.contains(name)) {
            return format!("_{name}");
        }
        if self.reg_names.contains(name) {
            if is_lhs && self.posedge_lhs {
                format!("_n_{name}")
            } else if self.fsm_mode {
                name.to_string()
            } else {
                format!("_{name}")
            }
        } else if self.let_names.contains(name) {
            // `let port_name = expr` is a port-driver: there is no separate
            // `_let_port_name` storage. Reads (and the LHS write inside the
            // synthesized comb assign) bind to the public port field.
            if self.port_names.contains(name) {
                name.to_string()
            } else if self.fsm_mode {
                name.to_string()
            } else {
                format!("_let_{name}")
            }
        } else if self.inst_names.contains(name) {
            format!("_inst_{name}")
        } else if self.port_names.contains(name)
            && self.vec_names.map_or(false, |s| s.contains(name))
        {
            // Vec-typed port: header exposes flattened `name_0..name_N-1`
            // scalars for external access, but the body indexes into the
            // internal `_name[N]` array. Without this branch, a body that
            // reads / writes `port_name[i]` lowered to `port_name[0]` —
            // referencing a name that doesn't exist in the C++ class.
            format!("_{name}")
        } else {
            name.to_string()
        }
    }

    /// Emit a signal read.
    /// • 65–128-bit input ports: VlWide<4> → _arch_u128 conversion
    /// • >128-bit input ports:   return VlWide<N> directly (same as internal type)
    pub(super) fn read_signal(&self, name: &str) -> String {
        let base = self.resolve_name(name, false);
        if self.wide_names.contains(name) && self.port_names.contains(name) {
            let bits = self.widths.get(name).copied().unwrap_or(0);
            if bits > 128 {
                // Internal and port both VlWide<N> — no conversion needed
                base
            } else {
                // 65–128 bit: port is VlWide<ceil(W/32)>, internal arithmetic
                // uses _arch_u128. Pass the real word count so a VlWide<3>
                // (66–96 bit) backing array is not read out of bounds.
                let words = wide_words(bits);
                format!("_arch_vl_to_u128({base}._data, {words})")
            }
        } else {
            base
        }
    }

    pub(super) fn vec_path_of_expr(&self, expr: &Expr) -> Option<String> {
        match &expr.kind {
            ExprKind::Ident(name) => Some(name.clone()),
            ExprKind::FieldAccess(base, field) => {
                if let ExprKind::Ident(base_name) = &base.kind {
                    if self.bus_ports.contains(base_name.as_str()) {
                        Some(format!("{}_{}", base_name, field.name))
                    } else {
                        Some(format!("{}.{}", base_name, field.name))
                    }
                } else if let Some(base_path) = self.vec_path_of_expr(base) {
                    Some(format!("{}.{}", base_path, field.name))
                } else {
                    None
                }
            }
            // Outer index of a 2D Vec (e.g. `Vec<Vec<Bus,N>,M>`): the result
            // is still a Vec, so propagate the base name. Used by
            // `expr_is_vec` so the *inner* subscript stays as C array
            // indexing instead of falling into bit-shift extraction.
            ExprKind::Index(base, _) => {
                if let ExprKind::Ident(name) = &base.kind {
                    if self
                        .vec_2d_names
                        .map_or(false, |s| s.contains(name.as_str()))
                    {
                        return Some(name.clone());
                    }
                }
                None
            }
            _ => None,
        }
    }

    pub(super) fn expr_is_vec(&self, expr: &Expr) -> bool {
        self.vec_path_of_expr(expr)
            .map(|name| self.vec_names.map_or(false, |s| s.contains(name.as_str())))
            .unwrap_or(false)
    }

    pub(super) fn expr_vec_size(&self, expr: &Expr) -> Option<u64> {
        self.vec_path_of_expr(expr)
            .and_then(|name| self.vec_sizes.and_then(|m| m.get(name.as_str()).copied()))
    }

    pub(super) fn with_decl_types(
        mut self,
        decl_types: &'a HashMap<String, TypeExpr>,
        struct_defs: &'a HashMap<String, Vec<(String, TypeExpr)>>,
    ) -> Self {
        self.decl_types = Some(decl_types);
        self.struct_defs = Some(struct_defs);
        self
    }
}

pub(super) fn infer_expr_width(expr: &Expr, ctx: &Ctx) -> u32 {
    match &expr.kind {
        ExprKind::Ident(name) => {
            if let Some(&w) = ctx.widths.get(name.as_str()) {
                w
            } else {
                eprintln!(
                    "warning: sim codegen: width of identifier '{}' unknown; \
                     defaulting to 8 — concat / shift positions derived from \
                     this may be incorrect",
                    name
                );
                8
            }
        }
        ExprKind::LatencyAt(inner, _) | ExprKind::SvaNext(_, inner) => infer_expr_width(inner, ctx),
        ExprKind::Literal(LitKind::Sized(w, _)) => *w,
        ExprKind::Literal(_) => 32,
        ExprKind::Bool(_) => 1,
        ExprKind::MethodCall(base, method, _) if method.name == "reverse" => {
            infer_expr_width(base, ctx)
        }
        ExprKind::MethodCall(_, method, args)
            if method.name == "trunc"
                || method.name == "zext"
                || method.name == "sext"
                || method.name == "resize" =>
        {
            if let Some(w) = args.first() {
                eval_width_in(w, ctx)
            } else {
                8
            }
        }
        ExprKind::BitSlice(_, hi, lo) => {
            let h = eval_width_in(hi, ctx);
            let l = eval_width_in(lo, ctx);
            h - l + 1
        }
        ExprKind::PartSelect(_, _, width, _) => eval_width_in(width, ctx),
        ExprKind::Cast(_, ty) => match ty.as_ref() {
            TypeExpr::UInt(w) => eval_width_in(w, ctx),
            TypeExpr::SInt(w) => eval_width_in(w, ctx),
            _ => 8,
        },
        ExprKind::Concat(parts) => parts.iter().map(|p| infer_expr_width(p, ctx)).sum(),
        ExprKind::Index(base, _) => {
            // For Vec<T, N>[i] the result width is element T's width.
            // For scalar UInt/SInt[i] (bit indexing), the result is 1 bit.
            // Pre-fix: Index fell through to default 8, which broke
            // concat width inference (e.g. `{20{instr[31]}}` reported as
            // 160 bits instead of 20, blowing past the 32-bit port type
            // and emitting a VlWide<6> RHS for a uint32_t port).
            if let Some(base_name) = ctx.vec_path_of_expr(base) {
                if ctx
                    .vec_names
                    .map_or(false, |s| s.contains(base_name.as_str()))
                {
                    // Vec element width: total port/reg/field width / element count.
                    let total = ctx.widths.get(base_name.as_str()).copied().unwrap_or(0);
                    let count = ctx
                        .vec_sizes
                        .and_then(|m| m.get(base_name.as_str()))
                        .copied()
                        .unwrap_or(0);
                    if count > 0 && total > 0 {
                        return (total as u64 / count) as u32;
                    }
                }
            }
            // Scalar bit index → 1 bit.
            1
        }
        ExprKind::Repeat(count, value) => {
            let n = eval_width(count);
            let w = infer_expr_width(value, ctx);
            n * w
        }
        ExprKind::Binary(op, lhs, rhs) => {
            match op {
                // Comparison and logical ops always produce 1-bit Bool
                BinOp::Eq
                | BinOp::Neq
                | BinOp::Lt
                | BinOp::Gt
                | BinOp::Lte
                | BinOp::Gte
                | BinOp::And
                | BinOp::Or => 1,
                // Bitwise ops: result width = max of operand widths
                BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => {
                    let lw = infer_expr_width(lhs, ctx);
                    let rw = infer_expr_width(rhs, ctx);
                    std::cmp::max(lw, rw)
                }
                // Shift ops: result width = left operand width
                BinOp::Shl | BinOp::Shr => infer_expr_width(lhs, ctx),
                // Arithmetic ops: result width = max of operand widths
                _ => {
                    let lw = infer_expr_width(lhs, ctx);
                    let rw = infer_expr_width(rhs, ctx);
                    std::cmp::max(lw, rw)
                }
            }
        }
        ExprKind::Unary(UnaryOp::Not, _) => 1,
        ExprKind::Unary(UnaryOp::RedAnd, _)
        | ExprKind::Unary(UnaryOp::RedOr, _)
        | ExprKind::Unary(UnaryOp::RedXor, _) => 1,
        ExprKind::Ternary(_, then_expr, _) => infer_expr_width(then_expr, ctx),
        ExprKind::Signed(inner) | ExprKind::Unsigned(inner) => infer_expr_width(inner, ctx),
        ExprKind::FieldAccess(base, field) => {
            // Struct field access: look up by "<base>.<field>" key the caller
            // populated from the struct decl. Covers two shapes:
            //   - `ctrl_r.mode`             — base is Ident
            //   - `ch_r[0].threshold`       — base is Index of Ident (Vec elem)
            // Both resolve to the same struct-field width regardless of which
            // element index is being accessed.
            let base_name = match &base.kind {
                ExprKind::Ident(name) => Some(name.as_str()),
                ExprKind::Index(b, _) => match &b.kind {
                    ExprKind::Ident(name) => Some(name.as_str()),
                    _ => None,
                },
                _ => None,
            };
            if let Some(name) = base_name {
                let key = format!("{}.{}", name, field.name);
                if let Some(&w) = ctx.widths.get(key.as_str()) {
                    return w;
                }
            }
            // Bus field — falls through to the flattened C++ name lookup.
            let flat = cpp_expr_inner(expr, ctx, false);
            if let Some(&w) = ctx.widths.get(flat.as_str()) {
                w
            } else {
                eprintln!(
                    "warning: sim codegen: width of field access '{}' unknown; \
                     defaulting to 8 — concat / shift positions derived from \
                     this may be incorrect",
                    flat
                );
                8
            }
        }
        _ => 8,
    }
}

pub(super) fn infer_expr_signed(expr: &Expr, ctx: &Ctx) -> bool {
    match &expr.kind {
        ExprKind::Ident(name) => ctx.signed_names.contains(name.as_str()),
        ExprKind::FieldAccess(base, field) => {
            if let ExprKind::Ident(base_name) = &base.kind {
                let flat = if ctx.bus_ports.contains(base_name.as_str()) {
                    format!("{}_{}", base_name, field.name)
                } else {
                    format!("{}.{}", base_name, field.name)
                };
                ctx.signed_names.contains(flat.as_str())
            } else {
                false
            }
        }
        ExprKind::Cast(_, ty) => matches!(ty.as_ref(), TypeExpr::SInt(_)),
        ExprKind::Signed(_) => true,
        ExprKind::Unsigned(_) => false,
        ExprKind::MethodCall(base, method, _)
            if matches!(
                method.name.as_str(),
                "trunc" | "sext" | "resize" | "reverse"
            ) =>
        {
            infer_expr_signed(base, ctx)
        }
        ExprKind::Unary(UnaryOp::Neg, _) => true,
        ExprKind::Unary(_, inner) => infer_expr_signed(inner, ctx),
        ExprKind::Binary(op, lhs, rhs) => match op {
            BinOp::Eq
            | BinOp::Neq
            | BinOp::Lt
            | BinOp::Gt
            | BinOp::Lte
            | BinOp::Gte
            | BinOp::And
            | BinOp::Or => false,
            _ => infer_expr_signed(lhs, ctx) || infer_expr_signed(rhs, ctx),
        },
        ExprKind::Ternary(_, then_expr, else_expr) => {
            infer_expr_signed(then_expr, ctx) || infer_expr_signed(else_expr, ctx)
        }
        _ => false,
    }
}

/// Floating-point format of a sim signal/expression. Mirrors `Ty::FP32`/`BF16`
/// but local to the sim backend so it can live in `Ctx`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FpFmt {
    Fp32,
    Bf16,
    E4m3,
    E5m2,
}

impl FpFmt {
    /// Suffix used in the `_arch_fp.h` helper names: `_arch_f32_add` / `_arch_bf16_add`.
    pub(super) fn helper_tag(self) -> &'static str {
        match self {
            FpFmt::Fp32 => "f32",
            FpFmt::Bf16 => "bf16",
            FpFmt::E4m3 => "e4m3",
            FpFmt::E5m2 => "e5m2",
        }
    }
}

/// Infer the floating-point format of an expression, or `None` if it is not a
/// float. Drives dispatch of `+ - *` / comparisons to the `_arch_fp.h` helpers.
pub(super) fn infer_expr_float(expr: &Expr, ctx: &Ctx) -> Option<FpFmt> {
    match &expr.kind {
        ExprKind::Ident(name) => ctx.float_names.get(name.as_str()).copied(),
        // Float literals default to FP32.
        ExprKind::Literal(LitKind::Float(_)) => Some(FpFmt::Fp32),
        // Already resolved against its context float type at compile time
        // (arch#622/#624).
        ExprKind::Literal(LitKind::TypedFloat(FloatLitFmt::Fp32, _)) => Some(FpFmt::Fp32),
        ExprKind::Literal(LitKind::TypedFloat(FloatLitFmt::Bf16, _)) => Some(FpFmt::Bf16),
        ExprKind::Literal(LitKind::TypedFloat(FloatLitFmt::E4m3, _)) => Some(FpFmt::E4m3),
        ExprKind::Literal(LitKind::TypedFloat(FloatLitFmt::E5m2, _)) => Some(FpFmt::E5m2),
        ExprKind::Cast(_, ty) => match ty.as_ref() {
            TypeExpr::FP32 => Some(FpFmt::Fp32),
            TypeExpr::BF16 => Some(FpFmt::Bf16),
            TypeExpr::FP8E4M3 => Some(FpFmt::E4m3),
            TypeExpr::FP8E5M2 => Some(FpFmt::E5m2),
            _ => None,
        },
        ExprKind::MethodCall(_, method, _) => match method.name.as_str() {
            "to_fp32" => Some(FpFmt::Fp32),
            "to_bf16" => Some(FpFmt::Bf16),
            "to_fp8e4m3" => Some(FpFmt::E4m3),
            "to_fp8e5m2" => Some(FpFmt::E5m2),
            _ => None, // to_uint/to_sint produce integers
        },
        // Arithmetic preserves the float format; comparisons are not float.
        ExprKind::Binary(op, lhs, rhs) => match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul => {
                infer_expr_float(lhs, ctx).or_else(|| infer_expr_float(rhs, ctx))
            }
            _ => None,
        },
        ExprKind::Ternary(_, then_expr, else_expr) => {
            infer_expr_float(then_expr, ctx).or_else(|| infer_expr_float(else_expr, ctx))
        }
        ExprKind::FunctionCall(name, args) if name == "fma" => {
            args.first().and_then(|a| infer_expr_float(a, ctx))
        }
        // Composite accesses: Vec element and struct field reads carry
        // their declared element/field float type (resolved through
        // `ctx.decl_types`/`ctx.struct_defs`).
        ExprKind::Index(..) | ExprKind::FieldAccess(..) => match sim_expr_decl_type(expr, ctx) {
            Some(TypeExpr::FP32) => Some(FpFmt::Fp32),
            Some(TypeExpr::BF16) => Some(FpFmt::Bf16),
            Some(TypeExpr::FP8E4M3) => Some(FpFmt::E4m3),
            Some(TypeExpr::FP8E5M2) => Some(FpFmt::E5m2),
            _ => None,
        },
        _ => None,
    }
}

/// Lower a Vec method call (any/all/count/contains/reduce_*) to an
/// unrolled C++ expression. Predicate identifier substitution for
/// `item`/`index` is done by building a fresh `Ctx` with `ident_subst`
/// pointing at the per-iteration map.
pub(super) fn lower_vec_method_cpp(
    recv_b: &str,
    recv: &Expr,
    method: &Ident,
    args: &[Expr],
    ctx: &Ctx,
) -> String {
    let n = match &recv.kind {
        ExprKind::Ident(n) => ctx.vec_sizes.and_then(|m| m.get(n)).copied(),
        _ => None,
    };
    let Some(n) = n else {
        return format!("{recv_b}.{}()", method.name);
    };
    let n_usize = n as usize;

    let emit_at = |i: u64| -> String {
        let mut sub: HashMap<String, String> = HashMap::new();
        sub.insert("item".to_string(), format!("{recv_b}[{i}]"));
        sub.insert("index".to_string(), format!("{i}"));
        let sub_ctx = Ctx {
            reg_names: ctx.reg_names,
            port_names: ctx.port_names,
            let_names: ctx.let_names,
            let_values: ctx.let_values,
            inst_names: ctx.inst_names,
            wide_names: ctx.wide_names,
            widths: ctx.widths,
            signed_names: ctx.signed_names,
            float_names: ctx.float_names,
            posedge_lhs: ctx.posedge_lhs,
            fsm_mode: ctx.fsm_mode,
            enum_map: ctx.enum_map,
            bus_ports: ctx.bus_ports,
            reset_levels: ctx.reset_levels,
            vec_names: ctx.vec_names,
            vec_2d_names: ctx.vec_2d_names,
            vec_sizes: ctx.vec_sizes,
            fsm_vec_port_regs: ctx.fsm_vec_port_regs,
            ident_subst: None, // replaced below via a temporary binding
            loop_var_subst: ctx.loop_var_subst,
            vec_of_bus_port_count: ctx.vec_of_bus_port_count,
            vec_of_bus_wire_count: ctx.vec_of_bus_wire_count,
            coverage: ctx.coverage,
            params: ctx.params,
            vinit_regs: ctx.vinit_regs,
            decl_types: ctx.decl_types,
            struct_defs: ctx.struct_defs,
        };
        // The sub map must outlive the cpp_expr call. We keep `sub` as a
        // stack-local binding whose lifetime covers the call.
        let ctx_with_sub = Ctx {
            ident_subst: Some(&sub),
            ..sub_ctx
        };
        if let Some(pred) = args.first() {
            cpp_expr(pred, &ctx_with_sub)
        } else {
            String::new()
        }
    };

    match method.name.as_str() {
        "any" => {
            if n_usize == 0 {
                return "false".to_string();
            }
            let terms: Vec<String> = (0..n as u64).map(emit_at).collect();
            format!("({})", terms.join(" || "))
        }
        "all" => {
            if n_usize == 0 {
                return "true".to_string();
            }
            let terms: Vec<String> = (0..n as u64).map(emit_at).collect();
            format!("({})", terms.join(" && "))
        }
        "count" => {
            if n_usize == 0 {
                return "0".to_string();
            }
            let terms: Vec<String> = (0..n as u64)
                .map(|i| format!("({} ? 1u : 0u)", emit_at(i)))
                .collect();
            format!("({})", terms.join(" + "))
        }
        "contains" => {
            let Some(x_expr) = args.first() else {
                return "false".to_string();
            };
            let x = cpp_expr(x_expr, ctx);
            if n_usize == 0 {
                return "false".to_string();
            }
            let terms: Vec<String> = (0..n as u64)
                .map(|i| format!("({recv_b}[{i}] == {x})"))
                .collect();
            format!("({})", terms.join(" || "))
        }
        "reduce_or" => {
            if n_usize == 0 {
                return "0".to_string();
            }
            let terms: Vec<String> = (0..n as u64).map(|i| format!("{recv_b}[{i}]")).collect();
            format!("({})", terms.join(" | "))
        }
        "reduce_and" => {
            if n_usize == 0 {
                return "0".to_string();
            }
            let terms: Vec<String> = (0..n as u64).map(|i| format!("{recv_b}[{i}]")).collect();
            format!("({})", terms.join(" & "))
        }
        "reduce_xor" => {
            if n_usize == 0 {
                return "0".to_string();
            }
            let terms: Vec<String> = (0..n as u64).map(|i| format!("{recv_b}[{i}]")).collect();
            format!("({})", terms.join(" ^ "))
        }
        _ => format!("{recv_b}.{}()", method.name),
    }
}

pub(super) fn cpp_expr(expr: &Expr, ctx: &Ctx) -> String {
    cpp_expr_inner(expr, ctx, false)
}

pub(super) fn cpp_condition(expr: &Expr, ctx: &Ctx) -> String {
    let cond = cpp_expr(expr, ctx);
    if is_fully_wrapped_in_parens(&cond) {
        cond
    } else {
        format!("({cond})")
    }
}

/// Check if `s` is fully wrapped in a single balanced pair of outer parens.
/// Returns true for `(!busy)` and `(a + b)`, false for `(uint8_t)(!busy)` where
/// the first `)` closes the cast, not the whole expression.
pub(super) fn is_fully_wrapped_in_parens(s: &str) -> bool {
    let s = s.trim();
    if !s.starts_with('(') || !s.ends_with(')') {
        return false;
    }
    let mut depth = 0u32;
    for (i, c) in s.char_indices() {
        if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth -= 1;
            if depth == 0 && i < s.len() - 1 {
                return false; // closed before the end — not fully wrapped
            }
        }
    }
    depth == 0
}

pub(super) fn cpp_expr_lhs(expr: &Expr, ctx: &Ctx) -> String {
    cpp_expr_inner(expr, ctx, true)
}

pub(super) fn cpp_expr_inner(expr: &Expr, ctx: &Ctx, is_lhs: bool) -> String {
    match &expr.kind {
        // See the matching arm in codegen/mod.rs::emit_expr_inner:
        // `pipelined_ops::lower_pipelined_calls` (proposal phase 3)
        // rewrites every codegen-backed `PipelinedCall` into a plain
        // `FunctionCall` before sim codegen starts, so this is a loud
        // backstop, not the primary error path.
        ExprKind::PipelinedCall(name, _, stages) => unreachable!(
            "sim codegen reached `{name}<pipelined, {stages}>(...)` — this should have been \
             lowered by pipelined_ops::lower_pipelined_calls before codegen started"
        ),
        // Latency annotation: transparent to sim emission. The assignment
        // site handles directing the write to stage 0 of the pipe chain;
        // reads of `q@0` collapse to the final-output field of the pipe.
        ExprKind::LatencyAt(inner, _) => cpp_expr_inner(inner, ctx, is_lhs),
        // SVA `##N expr` is sim-irrelevant (assert/cover bodies aren't
        // lowered to runtime checks); emit the inner expression as a
        // safe fallback in case it's ever reached via a non-assert path.
        ExprKind::SvaNext(_, inner) => cpp_expr_inner(inner, ctx, is_lhs),
        // SynthIdent: emit as a plain identifier. Simulation support for
        // credit_channel (counter + FIFO mirror in C++) is separate work;
        // designs that use method dispatch today work under `arch build`
        // but not under `arch sim` — the name will reference an undefined
        // C++ symbol at sim-compile time. Intentional: we surface the gap
        // loudly rather than silently succeed.
        ExprKind::SynthIdent(name, _) => name.clone(),
        ExprKind::Literal(lit) => match lit {
            LitKind::Dec(v) => format!("{v}"),
            LitKind::Hex(v) => format!("0x{v:X}"),
            LitKind::Bin(v) => format!("{v}"),
            LitKind::Sized(_, v) | LitKind::ParamSized(_, v) => format!("{v}"),
            // Float literals are FP32 by default — emit the binary32 bit pattern
            // as an unsigned hex constant (matches the uint32_t carrier).
            LitKind::Float(bits) => format!("0x{:X}u", (f64::from_bits(*bits) as f32).to_bits()),
            // Already rounded to its context float type at compile time
            // (arch#622/#624) — emit the exact bit pattern directly (fits
            // the uint16_t/uint32_t carrier by construction).
            LitKind::TypedFloat(_, bits) => format!("0x{bits:X}u"),
        },
        ExprKind::Bool(true) => "1".to_string(),
        ExprKind::Bool(false) => "0".to_string(),

        ExprKind::Ident(name) => {
            // Vec method predicate binder: `item` / `index` are rebound per
            // iteration by the enclosing `cpp_expr` Vec-method handler.
            if let Some(sub) = ctx.ident_subst.and_then(|m| m.get(name)) {
                return sub.clone();
            }
            // Static for-loop unroll binds the loop variable to a literal
            // integer (e.g. `chans[i].v` inside `for i in 0..N-1`).
            if let Some(v) = ctx
                .loop_var_subst
                .and_then(|c| c.borrow().get(name).copied())
            {
                return v.to_string();
            }
            if is_lhs {
                ctx.resolve_name(name, true)
            } else {
                ctx.read_signal(name)
            }
        }

        ExprKind::Binary(op, lhs, rhs) => {
            let l = cpp_expr(lhs, ctx);
            let r = cpp_expr(rhs, ctx);
            if *op == BinOp::Implies {
                return format!("(!{l} || {r})");
            }
            if *op == BinOp::ImpliesNext {
                // Sim shadow-reg lifting handles this at the assert site;
                // by the time it reaches expr lowering, lhs has been rewritten
                // into past-state. Treat as Implies for fallback paths.
                return format!("(!{l} || {r})");
            }
            // Floating-point operands: dispatch to the `_arch_fp.h` helpers
            // (IEEE-754 RNE) instead of integer operators on the bit pattern.
            if let Some(fmt) = infer_expr_float(lhs, ctx).or_else(|| infer_expr_float(rhs, ctx)) {
                let tag = fmt.helper_tag();
                let fop = match op {
                    BinOp::Add => Some("add"),
                    BinOp::Sub => Some("sub"),
                    BinOp::Mul => Some("mul"),
                    BinOp::Eq => Some("eq"),
                    BinOp::Neq => Some("ne"),
                    BinOp::Lt => Some("lt"),
                    BinOp::Gt => Some("gt"),
                    BinOp::Lte => Some("le"),
                    BinOp::Gte => Some("ge"),
                    _ => None,
                };
                if let Some(fop) = fop {
                    return format!("_arch_{tag}_{fop}({l}, {r})");
                }
            }
            if matches!(op, BinOp::Mul | BinOp::MulWrap) {
                // Native sim computes the product in a 128-bit intermediate
                // (`_arch_u128` / `__int128_t`). When the operation's own
                // result cannot fit in 128 bits the product is silently
                // truncated, so reject loudly instead.
                //   - plain `*`  : ARCH widens losslessly to W(lhs)+W(rhs);
                //                  the full product must fit in 128 bits.
                //   - `*%`       : result width = max(W(lhs), W(rhs)); only
                //                  unsupported when an operand itself exceeds
                //                  128 bits (a ≤128-bit modular result is
                //                  computed correctly — u128 holds its low
                //                  bits exactly).
                // `arch build` (SV) and `arch formal` (SMT) handle
                // arbitrary-width multiply correctly — only `arch sim` is
                // limited.
                let lw = infer_expr_width(lhs, ctx);
                let rw = infer_expr_width(rhs, ctx);
                let result_w = if *op == BinOp::MulWrap {
                    lw.max(rw)
                } else {
                    lw + rw
                };
                if result_w > 128 {
                    let opname = if *op == BinOp::MulWrap { "*%" } else { "*" };
                    eprintln!(
                        "error: native sim does not support `{opname}` whose result needs more than \
                         128 bits (this multiply needs {result_w} bits). The native C++ simulator \
                         computes products in a 128-bit integer; wider results are unsupported and \
                         would be silently truncated.\n  \
                         note: `arch build` (SystemVerilog) and `arch formal` (SMT-LIB2) handle \
                         this multiply correctly — only `arch sim` is affected.\n  \
                         help: keep the multiply's result within 128 bits (for a modular result \
                         use `*%`, e.g. `(a *% b).trunc<N>()`), or file an enhancement request for \
                         native-sim wide-multiply support: \
                         https://github.com/arch-hdl-lang/arch-com/issues"
                    );
                    std::process::exit(1);
                }
                let cast_ty = if infer_expr_signed(lhs, ctx) || infer_expr_signed(rhs, ctx) {
                    "__int128_t"
                } else {
                    "_arch_u128"
                };
                let product = format!("((({cast_ty})({l})) * (({cast_ty})({r})))");
                return if *op == BinOp::MulWrap {
                    let bits = infer_expr_width(expr, ctx);
                    if infer_expr_signed(expr, ctx) {
                        cast_to_signed_bits(&product, bits)
                    } else {
                        cast_to_bits(&product, bits)
                    }
                } else {
                    product
                };
            }
            let op_str = match op {
                BinOp::Add | BinOp::AddWrap => "+",
                BinOp::Sub | BinOp::SubWrap => "-",
                BinOp::Mul | BinOp::MulWrap => "*",
                BinOp::Div => "/",
                BinOp::Mod => "%",
                BinOp::Eq => "==",
                BinOp::Neq => "!=",
                BinOp::Lt => "<",
                BinOp::Gt => ">",
                BinOp::Lte => "<=",
                BinOp::Gte => ">=",
                BinOp::And => "&&",
                BinOp::Or => "||",
                BinOp::BitAnd => "&",
                BinOp::BitOr => "|",
                BinOp::BitXor => "^",
                BinOp::Shl => "<<",
                BinOp::Shr => ">>",
                BinOp::Implies | BinOp::ImpliesNext => unreachable!(),
            };
            // Runtime divide-by-zero check for / and % when the divisor is
            // not a compile-time-reducible constant. Literal zero is already
            // rejected at typecheck; non-zero literals / param-folded consts
            // need no runtime check. Only truly-runtime divisors are wrapped.
            if matches!(op, BinOp::Div | BinOp::Mod) && !is_const_reducible(rhs) {
                let loc = base_ident_name(rhs).unwrap_or("<div>");
                let op_name = if *op == BinOp::Div { "/" } else { "%" };
                return format!("(_ARCH_DCHK(({r}), \"{loc} {op_name}\"), ({l} {op_str} {r}))");
            }
            format!("({l} {op_str} {r})")
        }

        ExprKind::Unary(op, operand) => cpp_unary(op, operand, ctx),

        ExprKind::FieldAccess(base, field) => {
            if let ExprKind::Ident(base_name) = &base.kind {
                // rst.asserted — polarity-abstracted reset active check
                if field.name == "asserted" {
                    if let Some(level) = ctx.reset_levels.get(base_name.as_str()) {
                        let resolved = ctx.resolve_name(base_name, false);
                        return if *level == ResetLevel::Low {
                            format!("(!{resolved})")
                        } else {
                            resolved
                        };
                    }
                }
                // Bus port: itcm.cmd_valid → itcm_cmd_valid
                if ctx.bus_ports.contains(base_name.as_str()) {
                    let flat = format!("{}_{}", base_name, field.name);
                    return ctx.resolve_name(&flat, is_lhs);
                }
                if ctx.inst_names.contains(base_name.as_str()) {
                    return format!("_inst_{}.{}", base_name, field.name);
                }
            }
            // Indexed bus port: m_axi[0].valid → m_axi_0_valid. The index
            // may be a literal or a loop variable bound to a literal via
            // static for-loop unroll (see `loop_var_subst`).
            if let ExprKind::Index(arr, idx) = &base.kind {
                if let ExprKind::Ident(arr_name) = &arr.kind {
                    let idx_val: Option<u64> = match &idx.kind {
                        ExprKind::Literal(LitKind::Dec(i))
                        | ExprKind::Literal(LitKind::Hex(i))
                        | ExprKind::Literal(LitKind::Bin(i))
                        | ExprKind::Literal(LitKind::Sized(_, i)) => Some(*i),
                        ExprKind::Ident(loopvar) => ctx
                            .loop_var_subst
                            .and_then(|c| c.borrow().get(loopvar).copied())
                            .map(|v| v as u64),
                        _ => None,
                    };
                    if let Some(i) = idx_val {
                        let expanded = format!("{}_{}", arr_name, i);
                        if ctx.bus_ports.contains(expanded.as_str()) {
                            return format!("{}_{}_{}", arr_name, i, field.name);
                        }
                    }
                    // Variable (non-constant) index into a Vec<Bus>.
                    //
                    // The constant path above resolves to a per-element flat
                    // field (`o_0_valid`). Those fields are reference aliases
                    // into a real C array (`o_valid[N]` for ports,
                    // `_let_o[N]` struct array for wires), so a runtime index
                    // selects the right lane directly — the same packed-array
                    // form the SV emitter uses (`o_valid[sel]`). Without this
                    // the FieldAccess fell through to the scalar bit-select
                    // path below, mis-lowering `o[sel].valid` to
                    // `((o >> sel) & 1).valid` against an undefined `o`.
                    // Bounds-checked like every other runtime index.
                    if idx_val.is_none() {
                        let fld = &field.name;
                        if let Some(n) = ctx
                            .vec_of_bus_port_count
                            .and_then(|m| m.get(arr_name).copied())
                        {
                            let i = cpp_expr(idx, ctx);
                            return format!(
                                "(_ARCH_BCHK(({i}), {n}, \"{arr_name}[i].{fld}\"), {arr_name}_{fld}[{i}])"
                            );
                        }
                        if let Some(n) = ctx
                            .vec_of_bus_wire_count
                            .and_then(|m| m.get(arr_name).copied())
                        {
                            let i = cpp_expr(idx, ctx);
                            return format!(
                                "(_ARCH_BCHK(({i}), {n}, \"{arr_name}[i].{fld}\"), _let_{arr_name}[{i}].{fld})"
                            );
                        }
                    }
                }
            }
            // Use is_lhs when evaluating base so struct reg fields get _n_ prefix on LHS
            let b = cpp_expr_inner(base, ctx, is_lhs);
            format!("{b}.{}", field.name)
        }

        ExprKind::MethodCall(base, method, args) => cpp_method_call(base, method, args, ctx),

        ExprKind::Cast(inner, ty) => {
            let e = cpp_expr(inner, ctx);
            let t = cpp_port_type_with_params(ty, ctx.params);
            // For SInt casts whose source is narrower than the target
            // C++ int, sign-extend the value: a plain `(int64_t)x`
            // bit-cast leaves the upper bits zero, which makes a
            // would-be-negative N-bit value (where N < 64) appear as
            // a large positive int64_t. Subsequent `>>` on that
            // mis-typed value zero-fills instead of sign-extending.
            //
            // Standard idiom: shift left by (W_int - W_HDL) so the
            // HDL sign bit lands at the int's MSB, then arith-shift
            // right by the same amount to sign-extend.
            //
            // For UInt casts and same-width SInt casts, the bit-cast
            // is correct and we keep the original simple form.
            if let TypeExpr::SInt(w) = &**ty {
                let w_hdl = eval_width_in(w, ctx);
                let w_cpp: u32 = if w_hdl <= 8 {
                    8
                } else if w_hdl <= 16 {
                    16
                } else if w_hdl <= 32 {
                    32
                } else if w_hdl <= 64 {
                    64
                } else {
                    0
                }; // >64: VlWide / _arch_u128 paths
                let inner_w = infer_expr_width(inner, ctx);
                if w_cpp > 0 && inner_w > 0 && inner_w < w_cpp {
                    let shift = w_cpp - inner_w;
                    return format!("(({t})({e}) << {shift}) >> {shift}");
                }
            }
            format!("({t})({e})")
        }

        ExprKind::Index(base, idx) => {
            let b = cpp_expr_inner(base, ctx, is_lhs);
            let i = cpp_expr(idx, ctx);
            // Vec-typed regs/fields use C array subscript; scalar signals use bit extraction
            let is_vec = ctx.expr_is_vec(base);
            // Runtime bounds check (hard abort) — skip when index is a compile-time literal
            // since the type checker handles constant-bounds at compile time.
            let idx_is_const = matches!(&idx.kind, ExprKind::Literal(_));
            if is_vec {
                let limit = ctx.expr_vec_size(base).unwrap_or(0);
                if limit > 0 && !idx_is_const {
                    let loc = ctx
                        .vec_path_of_expr(base)
                        .unwrap_or_else(|| "<vec>".to_string());
                    format!("(_ARCH_BCHK(({i}), {limit}, \"{loc}\"), {b}[{i}])")
                } else {
                    format!("{b}[{i}]")
                }
            } else {
                let base_w = infer_expr_width(base, ctx);
                if base_w > 0 && !idx_is_const {
                    let loc = base_ident_name(base).unwrap_or("<bitsel>");
                    format!("(_ARCH_BCHK(({i}), {base_w}, \"{loc}[i]\"), ((({b}) >> ({i})) & 1))")
                } else {
                    format!("((({b}) >> ({i})) & 1)")
                }
            }
        }

        ExprKind::BitSlice(base, hi, lo) => {
            let b = cpp_expr(base, ctx);
            let h = eval_width_in(hi, ctx);
            let l = eval_width_in(lo, ctx);
            let base_w = infer_expr_width(base, ctx);
            // Static slice: hi/lo are compile-time. Bounds checked by typecheck.
            if base_w > 128 {
                // VlWide<N>: use word-array bit extractor
                let result_w = h - l + 1;
                let result_ty = if result_w <= 64 {
                    cpp_uint(result_w)
                } else {
                    "uint64_t"
                };
                format!("({result_ty})_arch_vw_bits({b}.data(), {h}, {l})")
            } else if base_w > 64 {
                bit_range_u128(&b, h, l)
            } else {
                bit_range(&b, h, l)
            }
        }

        ExprKind::PartSelect(base, start, width, up) => {
            cpp_part_select(base, start, width, *up, ctx)
        }

        ExprKind::EnumVariant(enum_name, variant) => {
            if let Some(variants) = ctx.enum_map.get(&enum_name.name) {
                let idx = variants
                    .iter()
                    .find(|(n, _)| *n == variant.name)
                    .map(|(_, v)| *v)
                    .unwrap_or(0);
                format!("{idx}")
            } else {
                // Previously this silently emitted `0` with a C++ comment,
                // which masked genuine bugs (e.g. missing enum in enum_map).
                // Emit an undeclared identifier so the C++ compiler surfaces
                // the problem with a clear symbol to grep for, and warn at
                // codegen time so it isn't missed in the noise.
                eprintln!(
                    "warning: sim codegen: enum {}::{} not found in enum map; \
                     emitting compile-error token",
                    enum_name.name, variant.name
                );
                format!(
                    "_ARCH_CODEGEN_ERROR_unknown_enum_{}_{}",
                    enum_name.name, variant.name
                )
            }
        }

        ExprKind::StructLiteral(name, fields) => {
            // Lower to an immediately-invoked lambda so the result is a proper
            // value of the struct type. Works regardless of whether the struct
            // has a user-declared default constructor.
            let sname = &name.name;
            let mut body = String::new();
            body.push_str(&format!("[&](){{ {sname} _t; "));
            for f in fields {
                let v = cpp_expr(&f.value, ctx);
                body.push_str(&format!("_t.{} = {v}; ", f.name.name));
            }
            body.push_str("return _t; }()");
            body
        }

        ExprKind::Todo => {
            // Per the spec, `todo!` compiles but aborts at sim runtime. The
            // old lowering (`"0 /* todo! */"`) compiled AND silently ran,
            // turning a placeholder into real zero behavior. Now a
            // comma-expression that prints a diagnostic and calls abort()
            // before yielding 0, so any `todo!` reached in simulation fails
            // loudly. abort() is available via verilated.h (includes
            // <cstdlib>).
            "(fprintf(stderr, \"ARCH: todo! reached at sim runtime\\n\"), abort(), 0)".to_string()
        }

        ExprKind::Concat(parts) => cpp_concat(parts, ctx),

        ExprKind::Repeat(count, value) => {
            // {N{expr}} — replicate expr N times by shift-OR
            let c = cpp_expr(count, ctx);
            let v = cpp_expr(value, ctx);
            let val_width = infer_expr_width(value, ctx);
            // Generate: _arch_repeat(val, count, val_width)
            format!("_arch_repeat((uint64_t)({v}), {c}, {val_width})")
        }
        ExprKind::Clog2(arg) => {
            let a = cpp_expr(arg, ctx);
            format!("_arch_clog2({a})")
        }
        ExprKind::Onehot(index) => {
            let idx = cpp_expr(index, ctx);
            format!("(1ULL << {idx})")
        }
        ExprKind::Signed(inner) => {
            // signed(x) reinterprets `x`'s bit pattern as a two's-complement
            // signed value. The bit pattern is unchanged, but C++ operators
            // (notably `>>`) behave differently for signed types: signed
            // right-shift sign-extends, unsigned right-shift zero-extends.
            //
            // The cast target is the smallest C++ signed int that fits
            // the HDL width: SInt<8> → int8_t, SInt<33> → int64_t, etc.
            // When the HDL width is STRICTLY LESS than the C++ int width
            // (e.g. SInt<33> → int64_t with 31 padding bits), a plain
            // bit-cast leaves those upper bits zero, so a value that
            // should be negative in HDL terms (HDL bit W-1 = 1) appears
            // POSITIVE in the C++ int, and a chained `>>` zero-fills the
            // upper bits instead of sign-extending. Sign-extend explicitly
            // by left-shifting the HDL sign bit to the C++ MSB and then
            // arith-shifting back: `((int_W)x << (W_cpp-W_hdl)) >> (W_cpp-W_hdl)`.
            // arch-ibex `IbexAlu`'s SRA uses exactly this pattern via
            // `signed({sign_ext_msb, 32b}) >> shamt`.
            let w = infer_expr_width(inner, ctx);
            let inner_c = cpp_expr(inner, ctx);
            if w == 0 || w > 64 {
                inner_c
            } else {
                let w_cpp: u32 = if w <= 8 {
                    8
                } else if w <= 16 {
                    16
                } else if w <= 32 {
                    32
                } else {
                    64
                };
                if w < w_cpp {
                    let pad = w_cpp - w;
                    format!("((({})({}) << {pad}) >> {pad})", cpp_sint(w), inner_c)
                } else {
                    format!("(({})({}))", cpp_sint(w), inner_c)
                }
            }
        }
        ExprKind::Unsigned(inner) => {
            // unsigned(x) is the inverse cast. Emit explicit unsigned cast
            // so a chained `>>` becomes a logical shift. (For values that
            // started unsigned this is a no-op, but `unsigned(signed(x) >> n)`
            // patterns rely on the cast to bring the result back to uint
            // for further unsigned operations.)
            let w = infer_expr_width(inner, ctx);
            let inner_c = cpp_expr(inner, ctx);
            if w == 0 || w > 64 {
                inner_c
            } else {
                format!("(({})({}))", cpp_uint(w), inner_c)
            }
        }

        ExprKind::Ternary(cond, then_expr, else_expr) => {
            let c = cpp_expr(cond, ctx);
            let t = cpp_expr(then_expr, ctx);
            let e = cpp_expr(else_expr, ctx);
            if let Some(reg) = ctx.coverage {
                let mut cov = reg.borrow_mut();
                let then_idx = cov.alloc(
                    "expr-then",
                    cond.span.start,
                    coverage_expr_label("then", cond),
                );
                let else_idx = cov.alloc(
                    "expr-else",
                    cond.span.start,
                    coverage_expr_label("else", cond),
                );
                return format!(
                    "(({c}) ? (_arch_cov[{then_idx}]++, ({t})) : (_arch_cov[{else_idx}]++, ({e})))"
                );
            }
            format!("(({c}) ? ({t}) : ({e}))")
        }

        ExprKind::FunctionCall(name, args) if name == "fma" && args.len() == 3 => {
            let fmt = infer_expr_float(&args[0], ctx)
                .or_else(|| infer_expr_float(&args[1], ctx))
                .or_else(|| infer_expr_float(&args[2], ctx))
                .unwrap_or(FpFmt::Fp32);
            let a = cpp_expr(&args[0], ctx);
            let b = cpp_expr(&args[1], ctx);
            let c = cpp_expr(&args[2], ctx);
            format!("_arch_fma_{}({a}, {b}, {c})", fmt.helper_tag())
        }
        ExprKind::FunctionCall(name, args) if name == "is_nan" && args.len() == 1 => {
            let fmt = infer_expr_float(&args[0], ctx).unwrap_or(FpFmt::Fp32);
            let a = cpp_expr(&args[0], ctx);
            format!("_arch_{}_isnan({a})", fmt.helper_tag())
        }
        ExprKind::FunctionCall(name, args) => {
            let arg_strs: Vec<String> = args.iter().map(|a| cpp_expr(a, ctx)).collect();
            format!("{name}({})", arg_strs.join(", "))
        }

        ExprKind::ExprMatch(scrutinee, arms) => {
            let s = cpp_expr(scrutinee, ctx);
            let mut result = "0".to_string();
            for arm in arms.iter().rev() {
                let val = cpp_expr(&arm.value, ctx);
                let cond = match &arm.pattern {
                    Pattern::Wildcard => {
                        result = val;
                        continue;
                    }
                    Pattern::Ident(id) => {
                        // Mirror Stmt::Match: if the ident names a let
                        // with a literal RHS, treat as `== <literal>`;
                        // else fall through as the ternary tail (default).
                        let folded = ctx
                            .let_values
                            .and_then(|m| m.get(&id.name))
                            .filter(|e| matches!(&e.kind, ExprKind::Literal(_)));
                        match folded {
                            Some(e) => {
                                let lit = cpp_expr(e, ctx);
                                format!("({s} == {lit})")
                            }
                            None => {
                                result = val;
                                continue;
                            }
                        }
                    }
                    Pattern::Literal(e) => {
                        let lit = cpp_expr(e, ctx);
                        format!("({s} == {lit})")
                    }
                    Pattern::EnumVariant(en, vr) => {
                        if let Some(variants) = ctx.enum_map.get(&en.name) {
                            let idx = variants
                                .iter()
                                .find(|(n, _)| *n == vr.name)
                                .map(|(_, v)| *v)
                                .unwrap_or(0);
                            format!("({s} == {idx})")
                        } else {
                            format!("({s} == 0)")
                        }
                    }
                };
                result = format!("({cond} ? {val} : {result})");
            }
            result
        }

        ExprKind::Match(scrutinee, _) => {
            format!("/* match({}) */ 0", cpp_expr(scrutinee, ctx))
        }

        ExprKind::Inside(scrutinee, members) => {
            let s = cpp_expr(scrutinee, ctx);
            let parts: Vec<String> = members
                .iter()
                .map(|m| match m {
                    InsideMember::Single(e) => {
                        let v = cpp_expr(e, ctx);
                        format!("({s} == {v})")
                    }
                    InsideMember::Range(lo, hi) => {
                        let l = cpp_expr(lo, ctx);
                        let h = cpp_expr(hi, ctx);
                        format!("({s} >= {l} && {s} <= {h})")
                    }
                })
                .collect();
            if parts.is_empty() {
                "0".to_string()
            } else {
                format!("({})", parts.join(" || "))
            }
        }
    }
}

pub(super) fn cpp_unary(op: &UnaryOp, operand: &Expr, ctx: &Ctx) -> String {
    let o = cpp_expr(operand, ctx);
    match op {
        UnaryOp::Not => format!("(!{o})"),
        UnaryOp::BitNot => {
            // Use logical ! (clamped to 0/1) only for 1-bit/Bool signals.
            // For wider types use bitwise ~.
            let is_one_bit = match &operand.kind {
                ExprKind::Ident(name) => ctx.widths.get(name.as_str()).copied().unwrap_or(32) == 1,
                _ => false,
            };
            if is_one_bit {
                format!("(uint8_t)(!({o}))")
            } else {
                format!("(~({o}))")
            }
        }
        UnaryOp::Neg => format!("(-{o})"),
        UnaryOp::RedAnd => {
            // Reduction AND: all bits set → 1
            let w = infer_expr_width(operand, ctx);
            if w > 128 {
                let words = wide_words(w);
                let last_bits = w % 32;
                let last_mask = if last_bits == 0 {
                    "0xFFFFFFFFU".to_string()
                } else {
                    format!("0x{:X}U", (1u32 << last_bits) - 1)
                };
                format!("[&](){{auto& _v={o};for(int _i=0;_i<{}-1;_i++)if(_v._data[_i]!=0xFFFFFFFFU)return(uint8_t)0;return(uint8_t)(_v._data[{}]=={last_mask}?1:0);}}()", words, words-1)
            } else if w <= 1 {
                format!("({o} & 1)")
            } else {
                let mask = if w >= 64 { u64::MAX } else { (1u64 << w) - 1 };
                format!("(uint8_t)(({o} & 0x{mask:x}ULL) == 0x{mask:x}ULL)")
            }
        }
        UnaryOp::RedOr => {
            // Reduction OR: any bit set → 1
            let w = infer_expr_width(operand, ctx);
            if w > 128 {
                let words = wide_words(w);
                format!("[&](){{auto& _v={o};for(int _i=0;_i<{words};_i++)if(_v._data[_i])return(uint8_t)1;return(uint8_t)0;}}()")
            } else {
                format!("(uint8_t)(({o}) != 0)")
            }
        }
        UnaryOp::RedXor => {
            // Reduction XOR: parity
            let w = infer_expr_width(operand, ctx);
            if w > 128 {
                let words = wide_words(w);
                format!("[&](){{auto& _v={o};uint8_t _p=0;for(int _i=0;_i<{words};_i++)_p^=(uint8_t)__builtin_parity(_v._data[_i]);return _p;}}()")
            } else {
                format!("(uint8_t)(__builtin_parityll((uint64_t)({o})))")
            }
        }
    }
}

pub(super) fn cpp_method_call(base: &Expr, method: &Ident, args: &[Expr], ctx: &Ctx) -> String {
    let b = cpp_expr(base, ctx);
    match method.name.as_str() {
        "trunc" => {
            if let Some(w_expr) = args.first() {
                let bits = eval_width_in(w_expr, ctx);
                let base_w = infer_expr_width(base, ctx);
                if base_w > 128 && bits <= 64 {
                    // VlWide → narrow: extract low bits via word array
                    format!(
                        "({})_arch_vw_bits({b}.data(), {}, 0)",
                        cpp_uint(bits),
                        bits - 1
                    )
                } else if infer_expr_signed(base, ctx) {
                    cast_to_signed_bits(&b, bits)
                } else {
                    cast_to_bits(&b, bits)
                }
            } else {
                b
            }
        }
        "zext" => {
            if let Some(w_expr) = args.first() {
                let bits = eval_width_in(w_expr, ctx);
                let base_w = infer_expr_width(base, ctx);
                if bits > 128 {
                    // Narrow → VlWide: use uint64_t constructor
                    let words = wide_words(bits);
                    format!("VlWide<{words}>(static_cast<uint64_t>({b}))")
                } else if base_w > 128 && bits <= 64 {
                    format!(
                        "({})_arch_vw_bits({b}.data(), {}, 0)",
                        cpp_uint(bits),
                        bits - 1
                    )
                } else {
                    format!("({})({})", cpp_uint(bits), b)
                }
            } else {
                b
            }
        }
        "sext" => {
            if let Some(w_expr) = args.first() {
                let dst_bits = eval_width_in(w_expr, ctx);
                let src_bits = infer_expr_width(base, ctx);
                if src_bits >= dst_bits || src_bits == 0 {
                    // No extension needed or unknown source width
                    format!("({})({})", cpp_uint(dst_bits), b)
                } else {
                    // Sign-extend: if MSB of source is set, fill upper bits with 1s
                    let dst_t = cpp_uint(dst_bits);
                    format!("(({b} >> {}) & 1 ? ({dst_t})({b}) | ({dst_t})(~(({dst_t})0) << {src_bits}) : ({dst_t})({b}))",
                        src_bits - 1)
                }
            } else {
                b
            }
        }
        "resize" => {
            // Direction-agnostic: sign-extend if narrowing to signed, zero-pad if widening unsigned
            if let Some(w_expr) = args.first() {
                let dst_bits = eval_width_in(w_expr, ctx);
                let src_bits = infer_expr_width(base, ctx);
                if src_bits >= dst_bits || src_bits == 0 {
                    // Narrowing or equal: just cast (C++ truncates)
                    cast_to_bits(&b, dst_bits)
                } else {
                    // Widening: zero-extend (same as zext for sim purposes)
                    format!("({})({})", cpp_uint(dst_bits), b)
                }
            } else {
                b
            }
        }
        "reverse" => {
            let base_w = infer_expr_width(base, ctx);
            let chunk = if let Some(c) = args.first() {
                eval_width_in(c, ctx)
            } else {
                1
            };
            if chunk == 1 {
                // Bit-reverse: build at compile time
                if base_w <= 64 {
                    format!("[&]() {{ {ty} v = {b}; {ty} r = 0; for (int i = 0; i < {w}; i++) r |= (({ty})((v >> i) & 1)) << ({w} - 1 - i); return r; }}()",
                        ty = cpp_uint(base_w), w = base_w)
                } else {
                    // Wide (>64 bit) reversal via VlWide
                    format!("[&]() {{ auto v = {b}; {ty} r{{}}; for (int i = 0; i < {w}; i++) {{ int sw = i / 32; int sb = i % 32; int dw = ({w} - 1 - i) / 32; int db = ({w} - 1 - i) % 32; if ((v[sw] >> sb) & 1) r[dw] |= (1u << db); }} return r; }}()",
                        ty = cpp_uint(base_w), w = base_w)
                }
            } else {
                // Chunk-reverse: reverse order of N-bit chunks
                let n_chunks = base_w / chunk;
                if base_w <= 64 {
                    format!("[&]() {{ {ty} v = {b}; {ty} r = 0; for (int i = 0; i < {nc}; i++) r |= ((v >> (i * {c})) & (({ty})((1ULL << {c}) - 1))) << (({nc} - 1 - i) * {c}); return r; }}()",
                        ty = cpp_uint(base_w), nc = n_chunks, c = chunk)
                } else {
                    // Wide chunk reverse — extract and place via bit loops
                    format!("[&]() {{ auto v = {b}; {ty} r{{}}; for (int ci = 0; ci < {nc}; ci++) for (int bi = 0; bi < {c}; bi++) {{ int si = ci * {c} + bi; int di = ({nc} - 1 - ci) * {c} + bi; int sw = si / 32; int sb = si % 32; int dw = di / 32; int db = di % 32; if ((v[sw] >> sb) & 1) r[dw] |= (1u << db); }} return r; }}()",
                        ty = cpp_uint(base_w), nc = n_chunks, c = chunk)
                }
            }
        }
        "any" | "all" | "count" | "contains" | "reduce_or" | "reduce_and" | "reduce_xor" => {
            lower_vec_method_cpp(&b, base, method, args, ctx)
        }
        // Float conversions → `_arch_fp.h` helpers.
        "to_fp32" => match infer_expr_float(base, ctx) {
            Some(FpFmt::Bf16) => format!("_arch_bf16_to_f32({b})"),
            Some(FpFmt::E4m3) => format!("_arch_e4m3_to_f32({b})"),
            Some(FpFmt::E5m2) => format!("_arch_e5m2_to_f32({b})"),
            Some(FpFmt::Fp32) => b, // no-op (typecheck rejects, but stay total)
            None => {
                if infer_expr_signed(base, ctx) {
                    format!("_arch_i_to_f32((int64_t)({b}))")
                } else {
                    format!("_arch_u_to_f32((uint64_t)({b}))")
                }
            }
        },
        "to_bf16" => match infer_expr_float(base, ctx) {
            Some(FpFmt::Fp32) => format!("_arch_f32_to_bf16({b})"),
            Some(FpFmt::Bf16) => b,
            // fp8 -> bf16: exact widen then exact narrow (every fp8 value
            // is exact in bf16).
            Some(FpFmt::E4m3) => format!("_arch_f32_to_bf16(_arch_e4m3_to_f32({b}))"),
            Some(FpFmt::E5m2) => format!("_arch_f32_to_bf16(_arch_e5m2_to_f32({b}))"),
            None => {
                if infer_expr_signed(base, ctx) {
                    format!("_arch_i_to_bf16((int64_t)({b}))")
                } else {
                    format!("_arch_u_to_bf16((uint64_t)({b}))")
                }
            }
        },
        "to_fp8e4m3" => match infer_expr_float(base, ctx) {
            Some(FpFmt::Fp32) => format!("_arch_f32_to_e4m3({b})"),
            Some(FpFmt::E4m3) => b,
            // BF16 / cross-fp8: exact widen, one narrow — correctly rounded.
            Some(FpFmt::Bf16) => format!("_arch_f32_to_e4m3(_arch_bf16_to_f32({b}))"),
            Some(FpFmt::E5m2) => format!("_arch_f32_to_e4m3(_arch_e5m2_to_f32({b}))"),
            // Integers: exact in f32 across the fp8-relevant range, so the
            // single fp8 rounding is correctly rounded.
            None => {
                if infer_expr_signed(base, ctx) {
                    format!("_arch_f32_to_e4m3(_arch_i_to_f32((int64_t)({b})))")
                } else {
                    format!("_arch_f32_to_e4m3(_arch_u_to_f32((uint64_t)({b})))")
                }
            }
        },
        "to_fp8e5m2" => match infer_expr_float(base, ctx) {
            Some(FpFmt::Fp32) => format!("_arch_f32_to_e5m2({b})"),
            Some(FpFmt::E5m2) => b,
            Some(FpFmt::Bf16) => format!("_arch_f32_to_e5m2(_arch_bf16_to_f32({b}))"),
            Some(FpFmt::E4m3) => format!("_arch_f32_to_e5m2(_arch_e4m3_to_f32({b}))"),
            None => {
                if infer_expr_signed(base, ctx) {
                    format!("_arch_f32_to_e5m2(_arch_i_to_f32((int64_t)({b})))")
                } else {
                    format!("_arch_f32_to_e5m2(_arch_u_to_f32((uint64_t)({b})))")
                }
            }
        },
        "to_uint" | "to_sint" => {
            let bits = args.first().map(|w| eval_width_in(w, ctx)).unwrap_or(32);
            let signed = method.name == "to_sint";
            // Decode bf16 to f32 bits first; then a width-aware, saturating,
            // toward-zero, NaN→type-max conversion to the N-bit integer.
            let f32bits = match infer_expr_float(base, ctx) {
                Some(FpFmt::Bf16) => format!("_arch_bf16_to_f32({b})"),
                Some(FpFmt::E4m3) => format!("_arch_e4m3_to_f32({b})"),
                Some(FpFmt::E5m2) => format!("_arch_e5m2_to_f32({b})"),
                _ => b,
            };
            let conv = if signed {
                format!("_arch_f32_to_sint({f32bits}, {bits})")
            } else {
                format!("_arch_f32_to_uint({f32bits}, {bits})")
            };
            let cast = if signed {
                cpp_sint(bits)
            } else {
                cpp_uint(bits)
            };
            format!("(({cast})({conv}))")
        }
        _ => format!("{b}.{}()", method.name),
    }
}

pub(super) fn cpp_part_select(
    base: &Expr,
    start: &Expr,
    width: &Expr,
    up: bool,
    ctx: &Ctx,
) -> String {
    let b = cpp_expr(base, ctx);
    let s = cpp_expr(start, ctx);
    let w = eval_width_in(width, ctx);
    let base_w = infer_expr_width(base, ctx);
    let result_ty = cpp_uint(w);
    // Runtime bounds check for variable part-selects:
    //   [+:]: bits [start .. start+W-1] must fit, so (start + W - 1) < base_W
    //   [-:]: bits [start-W+1 .. start], so start < base_W and start >= W-1
    // Skip when start is a constant.
    let start_is_const = matches!(&start.kind, ExprKind::Literal(_));
    let bchk = if base_w > 0 && !start_is_const {
        let loc = base_ident_name(base).unwrap_or("<partsel>");
        if up {
            format!("_ARCH_BCHK((({s}) + {w} - 1), {base_w}, \"{loc}[+:{w}]\"), ")
        } else {
            // [-:W]: need start < base_W AND start >= W-1.
            // Check (start + 1 - W) as signed → unsigned wrap makes this >= base_W if invalid.
            format!("_ARCH_BCHK(({s}), {base_w}, \"{loc}[-:{w}] start\"), _ARCH_BCHK(({w} - 1), (({s}) + 1), \"{loc}[-:{w}] underflow\"), ")
        }
    } else {
        String::new()
    };
    let core = if base_w > 128 {
        // VlWide<N>: use _arch_vw_bits with runtime start
        let hi_expr = if up {
            format!("(({s}) + {w} - 1)")
        } else {
            format!("({s})")
        };
        let lo_expr = if up {
            format!("({s})")
        } else {
            format!("(({s}) - {} + 1)", w)
        };
        format!("({result_ty})_arch_vw_bits({b}.data(), {hi_expr}, {lo_expr})")
    } else if base_w > 64 {
        let mask = (1u128 << w).wrapping_sub(1);
        let mask_str = format!("0x{:x}ULL", mask as u64);
        if up {
            format!("({result_ty})(({b} >> ({s})) & {mask_str})")
        } else {
            format!("({result_ty})(({b} >> (({s}) - {} + 1)) & {mask_str})", w)
        }
    } else {
        let mask = if w >= 64 { u64::MAX } else { (1u64 << w) - 1 };
        let mask_str = format!("0x{:x}ULL", mask);
        if up {
            format!("({result_ty})((uint64_t)({b}) >> ({s}) & {mask_str})")
        } else {
            format!(
                "({result_ty})((uint64_t)({b}) >> (({s}) - {} + 1) & {mask_str})",
                w
            )
        }
    };
    if bchk.is_empty() {
        core
    } else {
        format!("({bchk}{core})")
    }
}

pub(super) fn cpp_concat(parts: &[Expr], ctx: &Ctx) -> String {
    if parts.is_empty() {
        return "0".to_string();
    }
    // Compute widths for each part (MSB first)
    let part_widths: Vec<u32> = parts.iter().map(|p| infer_expr_width(p, ctx)).collect();
    let total: u32 = part_widths.iter().sum();

    if total > 128 {
        // Result is a VlWide<N>: build via OR-shifted parts in a lambda
        let words = wide_words(total);
        let mut stmts = Vec::new();
        let mut bit_offset = 0u32;
        for (i, part) in parts.iter().enumerate().rev() {
            let w = part_widths[i];
            let val = cpp_expr(part, ctx);
            // Each part is cast to uint64_t (narrow) then placed into VlWide
            stmts.push(format!(
                "_r = _r | (VlWide<{words}>(static_cast<uint64_t>({val})) << {bit_offset});"
            ));
            bit_offset += w;
        }
        format!(
            "[&]() -> VlWide<{words}> {{ VlWide<{words}> _r{{}}; {} return _r; }}()",
            stmts.join(" ")
        )
    } else {
        // Build expression: accumulate shifts from LSB (last part offset=0)
        let mut terms = Vec::new();
        let mut bit_offset = 0u32;
        for (i, part) in parts.iter().enumerate().rev() {
            let w = part_widths[i];
            let val = cpp_expr(part, ctx);
            if total > 64 {
                terms.push(format!("((_arch_u128)(uint64_t)({val}) << {bit_offset})"));
            } else {
                terms.push(format!("((uint64_t)({val}) << {bit_offset})"));
            }
            bit_offset += w;
        }
        format!("({})", terms.join(" | "))
    }
}

/// Declared TypeExpr of an lvalue-shaped expression (identifier, Vec
/// element select, struct field access — recursively, so `v[i].f` and
/// `s.f[i]` both resolve). Mirrors the SV codegen's `expr_decl_type`.
pub(super) fn sim_expr_decl_type(e: &Expr, ctx: &Ctx) -> Option<TypeExpr> {
    match &e.kind {
        ExprKind::Ident(name) => ctx.decl_types?.get(name.as_str()).cloned(),
        ExprKind::Index(base, _) => match sim_expr_decl_type(base, ctx)? {
            TypeExpr::Vec(elem, _) => Some(*elem),
            _ => None,
        },
        ExprKind::FieldAccess(base, field) => {
            // Compound "port.signal" bus keys take priority (bus ports have
            // no struct definition to resolve through).
            if let ExprKind::Ident(root) = &base.kind {
                if let Some(t) = ctx
                    .decl_types
                    .and_then(|d| d.get(&format!("{}.{}", root, field.name)))
                {
                    return Some(t.clone());
                }
            }
            let TypeExpr::Named(sname) = sim_expr_decl_type(base, ctx)? else {
                return None;
            };
            for (fname, fty) in ctx.struct_defs?.get(&sname.name)? {
                if fname == &field.name {
                    return Some(fty.clone());
                }
            }
            None
        }
        _ => None,
    }
}
