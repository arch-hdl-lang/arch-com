use crate::ast::*;
use crate::diagnostics::CompileError;
use crate::lexer::{Span, Token, TokenKind};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    /// Original source text, used for newline detection in one-line syntax disambiguation
    source: String,
    /// When true, `>` and `>=` are not treated as binary operators (inside type angle brackets)
    no_angle: bool,
    /// Default init/reset applied to reg declarations that omit those clauses.
    /// Set by `reg default: init <expr> reset <...>;` within a module/pipeline body.
    reg_defaults: Option<(Option<Expr>, RegReset)>,
    /// Default clock/edge for seq blocks. Set by `default seq on <clk> rising|falling;`.
    seq_default: Option<(Ident, ClockEdge)>,
}

impl Parser {
    pub fn new(tokens: Vec<Token>, source: &str) -> Self {
        Self { tokens, pos: 0, source: source.to_string(), no_angle: false, reg_defaults: None, seq_default: None }
    }

    /// Check if there's a newline in the source between two byte offsets.
    fn has_newline_between(&self, from: usize, to: usize) -> bool {
        self.source[from..to].contains('\n')
    }

    pub fn parse_source_file(&mut self) -> Result<SourceFile, CompileError> {
        let mut items = Vec::new();
        while !self.at_end() {
            items.push(self.parse_item()?);
        }
        Ok(SourceFile { items })
    }

    fn parse_item(&mut self) -> Result<Item, CompileError> {
        match self.peek_kind() {
            Some(TokenKind::Domain) => Ok(Item::Domain(self.parse_domain()?)),
            Some(TokenKind::Struct) => Ok(Item::Struct(self.parse_struct()?)),
            Some(TokenKind::Enum) => Ok(Item::Enum(self.parse_enum()?)),
            Some(TokenKind::Module) => Ok(Item::Module(self.parse_module()?)),
            Some(TokenKind::Fsm) => Ok(Item::Fsm(self.parse_fsm()?)),
            Some(TokenKind::Fifo) => Ok(Item::Fifo(self.parse_fifo()?)),
            Some(TokenKind::Ram) => Ok(Item::Ram(self.parse_ram()?)),
            Some(TokenKind::Counter) => Ok(Item::Counter(self.parse_counter()?)),
            Some(TokenKind::Arbiter) => Ok(Item::Arbiter(self.parse_arbiter()?)),
            Some(TokenKind::Regfile) => Ok(Item::Regfile(self.parse_regfile()?)),
            Some(TokenKind::Pipeline) => Ok(Item::Pipeline(self.parse_pipeline()?)),
            Some(TokenKind::Function) => Ok(Item::Function(self.parse_function()?)),
            Some(TokenKind::Linklist) => Ok(Item::Linklist(self.parse_linklist()?)),
            Some(TokenKind::Template) => Ok(Item::Template(self.parse_template()?)),
            Some(TokenKind::Synchronizer) => Ok(Item::Synchronizer(self.parse_synchronizer()?)),
            Some(TokenKind::Clkgate) => Ok(Item::Clkgate(self.parse_clkgate()?)),
            Some(TokenKind::Bus) => Ok(Item::Bus(self.parse_bus()?)),
            Some(TokenKind::Package) => Ok(Item::Package(self.parse_package()?)),
            Some(TokenKind::Use) => Ok(Item::Use(self.parse_use()?)),
            Some(other) => Err(CompileError::unexpected_token(
                "domain, struct, enum, module, fsm, fifo, ram, counter, arbiter, regfile, pipeline, function, linklist, template, synchronizer, clkgate, bus, package, or use",
                &other.to_string(),
                self.peek_span(),
            )),
            None => Err(CompileError::UnexpectedEof),
        }
    }

    // --- Domain ---
    fn parse_domain(&mut self) -> Result<DomainDecl, CompileError> {
        let start = self.expect(TokenKind::Domain)?.span;
        let name = self.expect_ident()?;
        let mut fields = Vec::new();
        while !self.check_end_keyword() {
            let field_name = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            let value = self.parse_expr()?;
            fields.push(DomainField {
                name: field_name,
                value,
            });
        }
        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Domain)?;
        let closing_name = self.expect_ident()?;
        if closing_name.name != name.name {
            return Err(CompileError::mismatched_closing(
                &name.name,
                &closing_name.name,
                closing_name.span,
            ));
        }
        Ok(DomainDecl {
            span: start.merge(closing_name.span),
            name,
            fields,
        })
    }

    // --- Struct ---
    fn parse_struct(&mut self) -> Result<StructDecl, CompileError> {
        let start = self.expect(TokenKind::Struct)?.span;
        let name = self.expect_ident()?;
        let mut fields = Vec::new();
        while !self.check_end_keyword() {
            let field_name = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            let ty = self.parse_type_expr()?;
            self.expect(TokenKind::Semi)?;
            fields.push(StructField {
                name: field_name,
                ty,
            });
        }
        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Struct)?;
        let closing_name = self.expect_ident()?;
        if closing_name.name != name.name {
            return Err(CompileError::mismatched_closing(
                &name.name,
                &closing_name.name,
                closing_name.span,
            ));
        }
        Ok(StructDecl {
            span: start.merge(closing_name.span),
            name,
            fields,
        })
    }

    // --- Bus ---
    fn parse_bus(&mut self) -> Result<BusDecl, CompileError> {
        let start = self.expect(TokenKind::Bus)?.span;
        let name = self.expect_ident()?;
        let mut params = Vec::new();
        let mut signals = Vec::new();
        let mut generates = Vec::new();
        while !self.check_end_keyword() {
            if self.check_param() {
                params.push(self.parse_param_decl()?);
            } else if self.check(TokenKind::GenerateIf) {
                generates.push(self.parse_bus_generate_if(start)?);
            } else {
                signals.push(self.parse_bus_signal(start)?);
            }
        }
        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Bus)?;
        let closing_name = self.expect_ident()?;
        if closing_name.name != name.name {
            return Err(CompileError::mismatched_closing(
                &name.name,
                &closing_name.name,
                closing_name.span,
            ));
        }
        Ok(BusDecl {
            span: start.merge(closing_name.span),
            name,
            params,
            signals,
            generates,
        })
    }

    fn parse_bus_signal(&mut self, parent_span: Span) -> Result<PortDecl, CompileError> {
        let sig_name = self.expect_ident()?;
        self.expect(TokenKind::Colon)?;
        let direction = if self.eat_contextual("in") {
            Direction::In
        } else if self.eat_contextual("out") {
            Direction::Out
        } else {
            return Err(CompileError::unexpected_token(
                "in or out",
                &self.peek_kind().map(|k| k.to_string()).unwrap_or("EOF".into()),
                self.peek_span(),
            ));
        };
        let ty = self.parse_type_expr()?;
        self.expect(TokenKind::Semi)?;
        let end_span = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(parent_span);
        Ok(PortDecl {
            name: sig_name,
            direction,
            ty,
            default: None,
            reg_info: None,
            bus_info: None,
            shared: None,
            span: parent_span.merge(end_span),
        })
    }

    fn parse_bus_generate_if(&mut self, parent_span: Span) -> Result<BusGenerateIf, CompileError> {
        let start = self.expect(TokenKind::GenerateIf)?.span;
        let cond = self.parse_expr()?;
        let mut then_signals = Vec::new();
        // Parse then-branch signals until end generate_if or generate_else
        while !self.check_bus_gen_end() {
            then_signals.push(self.parse_bus_signal(parent_span)?);
        }
        // Optional generate_else
        let else_signals = if self.check(TokenKind::GenerateElse) {
            self.advance();
            let mut sigs = Vec::new();
            while !(self.pos + 1 < self.tokens.len()
                && self.tokens[self.pos].kind == TokenKind::End
                && self.tokens[self.pos + 1].kind == TokenKind::GenerateIf)
            {
                sigs.push(self.parse_bus_signal(parent_span)?);
            }
            sigs
        } else {
            Vec::new()
        };
        self.expect(TokenKind::End)?;
        let end_span = self.expect(TokenKind::GenerateIf)?.span;
        Ok(BusGenerateIf {
            cond,
            then_signals,
            else_signals,
            span: start.merge(end_span),
        })
    }

    fn check_bus_gen_end(&self) -> bool {
        // Stop at `end generate_if` or `generate_else`
        if self.check(TokenKind::GenerateElse) { return true; }
        self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos].kind == TokenKind::End
            && self.tokens[self.pos + 1].kind == TokenKind::GenerateIf
    }

    // --- Enum ---
    fn parse_enum(&mut self) -> Result<EnumDecl, CompileError> {
        let start = self.expect(TokenKind::Enum)?.span;
        let name = self.expect_ident()?;
        let mut variants = Vec::new();
        let mut values = Vec::new();
        while !self.check_end_keyword() {
            let variant = self.expect_ident()?;
            // Optional explicit value: Variant = expr
            let value = if self.eat(TokenKind::Eq) {
                Some(self.parse_expr()?)
            } else {
                None
            };
            // Comma is optional before `end`
            if !self.check_end_keyword() {
                self.expect(TokenKind::Comma)?;
            }
            variants.push(variant);
            values.push(value);
        }
        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Enum)?;
        let closing_name = self.expect_ident()?;
        if closing_name.name != name.name {
            return Err(CompileError::mismatched_closing(
                &name.name,
                &closing_name.name,
                closing_name.span,
            ));
        }
        Ok(EnumDecl {
            span: start.merge(closing_name.span),
            name,
            variants,
            values,
        })
    }

    // --- Module ---
    fn parse_module(&mut self) -> Result<ModuleDecl, CompileError> {
        let start = self.expect(TokenKind::Module)?.span;
        let name = self.expect_ident()?;
        self.reg_defaults = None; // reset per-module
        self.seq_default = None;

        // Optional: `implements TemplateName`
        let implements = if self.check(TokenKind::Implements) {
            self.advance();
            Some(self.expect_ident()?)
        } else {
            None
        };

        let mut params = Vec::new();
        let mut ports = Vec::new();
        let mut body = Vec::new();
        let mut hooks: Vec<crate::ast::ModuleHookDecl> = Vec::new();
        let mut cdc_safe = false;

        while !self.check_end_keyword() {
            match self.peek_kind() {
                // `pragma cdc_safe;` — suppress CDC checks for this module
                Some(TokenKind::Ident(ref s)) if s == "pragma" => {
                    self.advance();
                    let pragma_name = self.expect_ident()?;
                    if pragma_name.name == "cdc_safe" {
                        cdc_safe = true;
                    } else {
                        return Err(CompileError::general(
                            &format!("unknown pragma `{}`", pragma_name.name),
                            pragma_name.span,
                        ));
                    }
                    self.expect(TokenKind::Semi)?;
                    continue;
                }
                _ if self.check_param() => params.push(self.parse_param_decl()?),
                Some(TokenKind::Port) => ports.push(self.parse_port_decl()?),
                Some(TokenKind::Reg) => {
                    if self.peek_default_at(1) {
                        self.parse_reg_default_decl()?;
                    } else {
                        body.push(ModuleBodyItem::RegDecl(self.parse_reg_decl()?));
                    }
                }
                Some(TokenKind::Seq) => {
                    body.push(ModuleBodyItem::RegBlock(self.parse_always_block()?));
                }
                Some(TokenKind::Latch) => {
                    body.push(ModuleBodyItem::LatchBlock(self.parse_latch_block()?));
                }
                Some(TokenKind::Comb) => {
                    body.push(ModuleBodyItem::CombBlock(self.parse_comb_block()?));
                }
                Some(TokenKind::Let) => {
                    body.push(ModuleBodyItem::LetBinding(self.parse_let_binding()?));
                }
                Some(TokenKind::Wire) => {
                    body.push(ModuleBodyItem::WireDecl(self.parse_wire_decl()?));
                }
                Some(TokenKind::Inst) => {
                    body.push(ModuleBodyItem::Inst(self.parse_inst()?));
                }
                Some(TokenKind::PipeReg) => {
                    body.push(ModuleBodyItem::PipeRegDecl(self.parse_pipe_reg_decl()?));
                }
                Some(TokenKind::GenerateFor) => {
                    body.push(ModuleBodyItem::Generate(self.parse_generate_for()?));
                }
                Some(TokenKind::GenerateIf) => {
                    body.push(ModuleBodyItem::Generate(self.parse_generate_if()?));
                }
                Some(TokenKind::Hook) => {
                    hooks.push(self.parse_module_hook_decl()?);
                }
                Some(TokenKind::Thread) => {
                    body.push(ModuleBodyItem::Thread(self.parse_thread_block()?));
                }
                Some(TokenKind::Ident(ref s)) if s == "resource" => {
                    body.push(ModuleBodyItem::Resource(self.parse_resource_decl()?));
                }
                Some(TokenKind::Default) => {
                    // `default seq on <clk> rising|falling;`
                    self.parse_seq_default_decl()?;
                }
                Some(TokenKind::Assert) | Some(TokenKind::Cover) => {
                    body.push(ModuleBodyItem::Assert(self.parse_assert_decl()?));
                }
                Some(TokenKind::Function) => {
                    body.push(ModuleBodyItem::Function(self.parse_function()?));
                }
                Some(other) => {
                    return Err(CompileError::unexpected_token(
                        "param, port, reg, seq, comb, let, inst, pipe_reg, generate_for, generate_if, thread, default, assert, cover, function, or hook",
                        &other.to_string(),
                        self.peek_span(),
                    ));
                }
                None => return Err(CompileError::UnexpectedEof),
            }
        }

        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Module)?;
        let closing_name = self.expect_ident()?;
        if closing_name.name != name.name {
            return Err(CompileError::mismatched_closing(
                &name.name,
                &closing_name.name,
                closing_name.span,
            ));
        }

        Ok(ModuleDecl {
            span: start.merge(closing_name.span),
            name,
            params,
            ports,
            body,
            implements,
            hooks,
            cdc_safe,
        })
    }

    fn parse_param_decl(&mut self) -> Result<ParamDecl, CompileError> {
        // Check for `local param` prefix (local is a contextual keyword)
        let is_local = self.check_contextual("local");
        if is_local { self.advance(); }
        let start = self.expect(TokenKind::Param)?.span;
        let name = self.expect_ident()?;
        // Optional width qualifier: param NAME[hi:lo]: const
        let width_range = if self.eat(TokenKind::LBracket) {
            let hi = self.parse_expr()?;
            self.expect(TokenKind::Colon)?;
            let lo = self.parse_expr()?;
            self.expect(TokenKind::RBracket)?;
            Some((hi, lo))
        } else {
            None
        };
        self.expect(TokenKind::Colon)?;
        let kind = if self.eat(TokenKind::Const) {
            if let Some((hi, lo)) = width_range {
                ParamKind::WidthConst(hi, lo)
            } else {
                ParamKind::Const
            }
        } else if self.check(TokenKind::Type) {
            self.advance();
            self.expect(TokenKind::Eq)?;
            let ty = self.parse_type_expr()?;
            ParamKind::Type(ty)
        } else if matches!(self.peek_kind(), Some(TokenKind::Ident(_))) {
            // Enum-typed const: param MODE: EnumName = EnumName::Variant
            let enum_name = self.expect_ident()?;
            ParamKind::EnumConst(enum_name.name)
        } else {
            ParamKind::Const
        };
        let default = if self.eat(TokenKind::Eq) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect(TokenKind::Semi)?;
        let end_span = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(start);
        Ok(ParamDecl {
            name,
            kind,
            default,
            is_local,
            span: start.merge(end_span),
        })
    }

    fn parse_port_decl(&mut self) -> Result<PortDecl, CompileError> {
        let start = self.expect(TokenKind::Port)?.span;
        let is_reg = self.eat(TokenKind::Reg);
        let name = self.expect_ident()?;
        self.expect(TokenKind::Colon)?;
        // Check for bus port: `port name: initiator/target BusName<...>;`
        let bus_perspective = if self.eat_contextual("initiator") {
            Some(BusPerspective::Initiator)
        } else if self.eat_contextual("target") {
            Some(BusPerspective::Target)
        } else {
            None
        };
        if let Some(perspective) = bus_perspective {
            let bus_name = self.expect_ident()?;
            // Optional param assignments: <PARAM=val, ...>
            let params = if self.check(TokenKind::Lt) {
                self.advance();
                let old_no_angle = self.no_angle;
                self.no_angle = true;
                let mut assigns = Vec::new();
                loop {
                    let pname = self.expect_ident()?;
                    self.expect(TokenKind::Eq)?;
                    let pval = self.parse_expr()?;
                    assigns.push(ParamAssign { name: pname, value: pval });
                    if !self.eat(TokenKind::Comma) { break; }
                }
                self.no_angle = old_no_angle;
                self.expect(TokenKind::Gt)?;
                assigns
            } else {
                Vec::new()
            };
            self.expect(TokenKind::Semi)?;
            let end_span = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(start);
            return Ok(PortDecl {
                name,
                direction: Direction::Out, // placeholder; actual directions from bus decl
                ty: TypeExpr::Named(bus_name.clone()),
                default: None,
                reg_info: None,
                bus_info: Some(BusPortInfo { bus_name, perspective, params }),
                shared: None,
                span: start.merge(end_span),
            });
        }

        let direction = if self.eat_contextual("in") {
            Direction::In
        } else if self.eat_contextual("out") {
            Direction::Out
        } else {
            return Err(CompileError::unexpected_token(
                "in, out, initiator, or target",
                &self.peek_kind().map(|k| k.to_string()).unwrap_or("EOF".into()),
                self.peek_span(),
            ));
        };
        if is_reg && direction == Direction::In {
            return Err(CompileError::general(
                "port reg must be an output port",
                start.merge(self.peek_span()),
            ));
        }
        let ty = self.parse_type_expr()?;

        // For `port reg`, parse optional guard/init/reset (same syntax as `reg` decl).
        let reg_info = if is_reg {
            // `guard <sig>` — valid-signal guard (structural qualifier, comes first).
            let guard = if self.check(TokenKind::Guard) {
                self.advance();
                Some(self.expect_ident()?)
            } else {
                None
            };
            let init = if self.check(TokenKind::Init) {
                self.advance();
                Some(self.parse_expr()?)
            } else if let Some((default_init, _)) = &self.reg_defaults {
                default_init.clone()
            } else {
                None
            };
            let reset = if self.check_ident("reset") {
                self.advance();
                self.parse_reset_clause()?
            } else if let Some((_, default_reset)) = &self.reg_defaults {
                default_reset.clone()
            } else {
                RegReset::None
            };
            Some(PortRegInfo { init, reset, guard })
        } else {
            None
        };

        let default = if self.eat(TokenKind::Default) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        // Parse optional `shared(or|and)` annotation
        let shared = if self.check_ident("shared") {
            self.advance();
            self.expect(TokenKind::LParen)?;
            let reduction = if self.check(TokenKind::Or) {
                self.advance();
                SharedReduction::Or
            } else if self.check(TokenKind::And) {
                self.advance();
                SharedReduction::And
            } else {
                return Err(CompileError::unexpected_token(
                    "or or and",
                    &self.peek_kind().map(|k| k.to_string()).unwrap_or("EOF".into()),
                    self.peek_span(),
                ));
            };
            self.expect(TokenKind::RParen)?;
            Some(reduction)
        } else {
            None
        };
        self.expect(TokenKind::Semi)?;
        let end_span = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(start);
        Ok(PortDecl {
            name,
            direction,
            ty,
            default,
            reg_info,
            bus_info: None,
            shared,
            span: start.merge(end_span),
        })
    }


    /// Return true if the token `offset` positions ahead is `TokenKind::Default`.
    fn peek_default_at(&self, offset: usize) -> bool {
        self.tokens.get(self.pos + offset)
            .map(|t| matches!(t.kind, TokenKind::Default))
            .unwrap_or(false)
    }

    /// Parse the reset clause shared by default and normal reg declarations.
    /// Caller has already consumed the `reset` pseudo-keyword.
    fn parse_reset_clause(&mut self) -> Result<RegReset, CompileError> {
        if self.eat(TokenKind::None) {
            return Ok(RegReset::None);
        }
        let rst_signal = self.expect_ident()?;

        // Parse `=> VALUE` — required reset value
        self.expect(TokenKind::FatArrow)?;
        let reset_value = self.parse_expr()?;

        if self.check(TokenKind::Sync) || self.check(TokenKind::Async) {
            let kind = if self.eat(TokenKind::Sync) {
                ResetKind::Sync
            } else {
                self.advance();
                ResetKind::Async
            };
            let level = if self.eat(TokenKind::High) {
                ResetLevel::High
            } else if self.eat(TokenKind::Low) {
                ResetLevel::Low
            } else {
                return Err(CompileError::unexpected_token(
                    "high or low",
                    &self.peek_kind().map(|k| k.to_string()).unwrap_or("EOF".into()),
                    self.peek_span(),
                ));
            };
            Ok(RegReset::Explicit(rst_signal, kind, level, reset_value))
        } else {
            Ok(RegReset::Inherit(rst_signal, reset_value))
        }
    }

    /// Parse `reg default: [init <expr>] [reset <signal>=<value>] ;`
    fn parse_reg_default_decl(&mut self) -> Result<(), CompileError> {
        self.expect(TokenKind::Reg)?;
        self.expect(TokenKind::Default)?;
        self.expect(TokenKind::Colon)?;
        let init = if self.check(TokenKind::Init) {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };
        let reset = if self.check_ident("reset") {
            self.advance();
            self.parse_reset_clause()?
        } else {
            RegReset::None
        };
        self.expect(TokenKind::Semi)?;
        self.reg_defaults = Some((init, reset));
        Ok(())
    }

    fn parse_wire_decl(&mut self) -> Result<WireDecl, CompileError> {
        let start = self.expect(TokenKind::Wire)?.span;
        let name = self.expect_ident()?;
        self.expect(TokenKind::Colon)?;
        let ty = self.parse_type_expr()?;
        self.expect(TokenKind::Semi)?;
        let end_span = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(start);
        Ok(WireDecl {
            name,
            ty,
            span: start.merge(end_span),
        })
    }

    fn parse_reg_decl(&mut self) -> Result<RegDecl, CompileError> {
        let start = self.expect(TokenKind::Reg)?.span;
        let name = self.expect_ident()?;
        self.expect(TokenKind::Colon)?;
        let ty = self.parse_type_expr()?;

        // Optional `guard <sig>` — valid-signal guard annotation. Comes right after
        // TYPE, before init/reset, because it's a structural qualifier about the reg.
        let guard = if self.check(TokenKind::Guard) {
            self.advance();
            Some(self.expect_ident()?)
        } else {
            None
        };

        // `init` clause is optional — provides SV declaration initializer only.
        let init = if self.check(TokenKind::Init) {
            self.advance();
            Some(self.parse_expr()?)
        } else if let Some((default_init, _)) = &self.reg_defaults {
            default_init.clone()
        } else {
            None
        };

        // `reset` clause is optional when reg_defaults provides one.
        // New syntax: `reset rst=VALUE` where =VALUE is required.
        let reset = if self.check_ident("reset") {
            self.advance();
            self.parse_reset_clause()?
        } else if let Some((_, default_reset)) = &self.reg_defaults {
            default_reset.clone()
        } else {
            RegReset::None
        };

        self.expect(TokenKind::Semi)?;
        let end_span = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(start);
        Ok(RegDecl {
            name,
            ty,
            init,
            reset,
            guard,
            span: start.merge(end_span),
        })
    }

    /// Parse `default seq on <clk> rising|falling;`
    fn parse_seq_default_decl(&mut self) -> Result<(), CompileError> {
        self.expect(TokenKind::Default)?;
        self.expect(TokenKind::Seq)?;
        self.expect(TokenKind::On)?;
        let clock = self.expect_ident()?;
        let clock_edge = if self.eat(TokenKind::Rising) {
            ClockEdge::Rising
        } else if self.eat(TokenKind::Falling) {
            ClockEdge::Falling
        } else {
            return Err(CompileError::unexpected_token(
                "rising or falling",
                &self.peek_kind().map(|k| k.to_string()).unwrap_or("EOF".into()),
                self.peek_span(),
            ));
        };
        self.expect(TokenKind::Semi)?;
        self.seq_default = Some((clock, clock_edge));
        Ok(())
    }

    fn parse_always_block(&mut self) -> Result<RegBlock, CompileError> {
        let start = self.expect(TokenKind::Seq)?.span;

        // Explicit clock: `seq on clk rising/falling ...`
        if self.check(TokenKind::On) {
            self.advance(); // consume `on`
            let clock = self.expect_ident()?;
            let clock_edge = if self.eat(TokenKind::Rising) {
                ClockEdge::Rising
            } else if self.eat(TokenKind::Falling) {
                ClockEdge::Falling
            } else {
                return Err(CompileError::unexpected_token(
                    "rising or falling",
                    &self.peek_kind().map(|k| k.to_string()).unwrap_or("EOF".into()),
                    self.peek_span(),
                ));
            };

            let mut stmts = Vec::new();
            while !self.check_end_always() {
                stmts.push(self.parse_reg_stmt()?);
            }
            self.expect(TokenKind::End)?;
            self.expect(TokenKind::Seq)?;
            let end_span = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(start);
            return Ok(RegBlock { clock, clock_edge, stmts, span: start.merge(end_span) });
        }

        // No `on` — use default clock.
        let (clock, clock_edge) = self.seq_default.clone().ok_or_else(|| {
            CompileError::general(
                "`seq` without `on <clk>` requires `default seq on <clk> rising|falling;`",
                start,
            )
        })?;

        let mut stmts = Vec::new();
        while !self.check_end_always() {
            stmts.push(self.parse_reg_stmt()?);
        }
        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Seq)?;
        let end_span = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(start);
        Ok(RegBlock { clock, clock_edge, stmts, span: start.merge(end_span) })
    }

    fn check_end_always(&self) -> bool {
        self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos].kind == TokenKind::End
            && self.tokens[self.pos + 1].kind == TokenKind::Seq
    }

    // --- Thread ---

    /// Parse `thread [once] [Name] on CLK rising|falling, RST high|low ... end thread [Name]`
    fn parse_thread_block(&mut self) -> Result<ThreadBlock, CompileError> {
        let start = self.expect(TokenKind::Thread)?.span;

        // Optional `once`
        let once = self.check_ident("once");
        if once { self.advance(); }

        // Optional name — peek: if we see Ident followed by `on`, it's a name.
        // If we see `on` directly, no name.
        let name = if self.check(TokenKind::On) {
            None
        } else {
            Some(self.expect_ident()?)
        };

        // Clock clause: `on CLK rising|falling`
        self.expect(TokenKind::On)?;
        let clock = self.expect_ident()?;
        let clock_edge = if self.eat(TokenKind::Rising) {
            ClockEdge::Rising
        } else if self.eat(TokenKind::Falling) {
            ClockEdge::Falling
        } else {
            return Err(CompileError::unexpected_token(
                "rising or falling",
                &self.peek_kind().map(|k| k.to_string()).unwrap_or("EOF".into()),
                self.peek_span(),
            ));
        };

        // Reset clause: `, RST high|low`
        self.expect(TokenKind::Comma)?;
        let reset = self.expect_ident()?;
        let reset_level = if self.eat(TokenKind::High) {
            ResetLevel::High
        } else if self.eat(TokenKind::Low) {
            ResetLevel::Low
        } else {
            return Err(CompileError::unexpected_token(
                "high or low",
                &self.peek_kind().map(|k| k.to_string()).unwrap_or("EOF".into()),
                self.peek_span(),
            ));
        };

        // Optional `default when <cond> ... end default` — must come first in the body.
        let default_when = if self.check(TokenKind::Default) {
            let _kw = self.advance(); // consume `default`
            self.expect(TokenKind::When)?;
            let cond = self.parse_expr()?;
            let mut dw_stmts = Vec::new();
            while !(self.pos + 1 < self.tokens.len()
                && self.tokens[self.pos].kind == TokenKind::End
                && self.tokens[self.pos + 1].kind == TokenKind::Default)
            {
                dw_stmts.push(self.parse_thread_stmt()?);
            }
            self.expect(TokenKind::End)?;
            self.expect(TokenKind::Default)?;
            Some((cond, dw_stmts))
        } else {
            None
        };

        // Body
        let mut body = Vec::new();
        while !self.check_end_thread() {
            body.push(self.parse_thread_stmt()?);
        }

        // `end thread [Name]`
        self.expect(TokenKind::End)?;
        let end_kw_span = self.expect(TokenKind::Thread)?.span;

        // If named, consume and verify closing name; if `once`, also consume closing `once`
        let end_span;
        if let Some(ref n) = name {
            let closing_name = self.expect_ident()?;
            if closing_name.name != n.name {
                return Err(CompileError::mismatched_closing(
                    &n.name, &closing_name.name, closing_name.span,
                ));
            }
            end_span = closing_name.span;
        } else if once {
            // `end thread once`
            self.expect_contextual("once")?;
            end_span = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(end_kw_span);
        } else {
            end_span = end_kw_span;
        }

        Ok(ThreadBlock {
            name,
            clock,
            clock_edge,
            reset,
            reset_level,
            once,
            default_when,
            body,
            span: start.merge(end_span),
        })
    }

    fn check_end_thread(&self) -> bool {
        self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos].kind == TokenKind::End
            && self.tokens[self.pos + 1].kind == TokenKind::Thread
    }

    /// Parse a single statement inside a thread block.
    fn parse_thread_stmt(&mut self) -> Result<ThreadStmt, CompileError> {
        // `if` → thread if/else
        if self.check(TokenKind::If) {
            return self.parse_thread_if();
        }

        // `fork ... and ... join`
        if self.check_ident("fork") {
            return self.parse_thread_fork_join();
        }

        // `for var in start..end ... end for`
        if self.check(TokenKind::For) {
            return self.parse_thread_for();
        }

        // `lock resource_name ... end lock resource_name`
        if self.check_ident("lock") {
            return self.parse_thread_lock();
        }

        // `do ... until cond;` — hold comb outputs while waiting
        if self.check_ident("do") {
            return self.parse_thread_do_until();
        }

        // `log(...)` — debug output statement
        if self.check(TokenKind::Log) {
            return Ok(ThreadStmt::Log(self.parse_log_stmt()?));
        }

        // `wait` (contextual keyword)
        if self.check_ident("wait") {
            let wait_start = self.advance().span;
            // `wait until expr;`
            if self.check_ident("until") {
                self.advance();
                let cond = self.parse_expr()?;
                let semi_span = self.expect(TokenKind::Semi)?.span;
                return Ok(ThreadStmt::WaitUntil(cond, wait_start.merge(semi_span)));
            }
            // `wait N cycle;`
            let count = self.parse_expr()?;
            self.expect_contextual("cycle")?;
            let semi_span = self.expect(TokenKind::Semi)?.span;
            return Ok(ThreadStmt::WaitCycles(count, wait_start.merge(semi_span)));
        }

        // Assignment: `target = expr;` (comb) or `target <= expr;` (seq)
        let target = self.parse_expr()?;
        if self.eat(TokenKind::Eq) {
            let value = self.parse_expr()?;
            let semi_span = self.expect(TokenKind::Semi)?.span;
            let span = target.span.merge(semi_span);
            Ok(ThreadStmt::CombAssign(CombAssign { target, value, span }))
        } else if self.eat(TokenKind::LtEq) {
            let value = self.parse_expr()?;
            let semi_span = self.expect(TokenKind::Semi)?.span;
            let span = target.span.merge(semi_span);
            Ok(ThreadStmt::SeqAssign(RegAssign { target, value, span }))
        } else {
            Err(CompileError::unexpected_token(
                "= or <=",
                &self.peek_kind().map(|k| k.to_string()).unwrap_or("EOF".into()),
                self.peek_span(),
            ))
        }
    }

    /// Parse `if ... elsif ... else ... end if` inside a thread block.
    fn parse_thread_if(&mut self) -> Result<ThreadStmt, CompileError> {
        let start = self.expect(TokenKind::If)?.span;
        let cond = self.parse_expr()?;

        let mut then_stmts = Vec::new();
        while !self.check_end_if() && !self.check(TokenKind::Else) && !self.check(TokenKind::ElsIf) {
            then_stmts.push(self.parse_thread_stmt()?);
        }

        let mut else_stmts = Vec::new();
        if self.check(TokenKind::ElsIf) {
            self.tokens[self.pos].kind = TokenKind::If;
            let nested = self.parse_thread_if()?;
            else_stmts.push(nested);
        } else if self.check(TokenKind::Else) {
            self.advance();
            while !self.check_end_if() {
                else_stmts.push(self.parse_thread_stmt()?);
            }
            self.expect(TokenKind::End)?;
            self.expect(TokenKind::If)?;
        } else {
            self.expect(TokenKind::End)?;
            self.expect(TokenKind::If)?;
        }

        let end_span = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(start);
        Ok(ThreadStmt::IfElse(ThreadIfElse {
            cond,
            then_stmts,
            else_stmts,
            unique: false,
            span: start.merge(end_span),
        }))
    }

    /// Parse `fork ... and ... join` inside a thread block.
    fn parse_thread_fork_join(&mut self) -> Result<ThreadStmt, CompileError> {
        let start = self.expect_contextual("fork")?.span;
        let mut branches: Vec<Vec<ThreadStmt>> = Vec::new();

        // Parse first branch
        let mut branch = Vec::new();
        loop {
            // Check for `and` (branch separator) or `join` (end)
            if self.check(TokenKind::And) {
                self.advance();
                branches.push(std::mem::take(&mut branch));
                continue;
            }
            if self.check_ident("join") {
                let end_span = self.advance().span;
                branches.push(std::mem::take(&mut branch));
                return Ok(ThreadStmt::ForkJoin(branches, start.merge(end_span)));
            }
            branch.push(self.parse_thread_stmt()?);
        }
    }

    /// Parse `for var in start..end ... end for` inside a thread block.
    fn parse_thread_for(&mut self) -> Result<ThreadStmt, CompileError> {
        let start = self.expect(TokenKind::For)?.span;
        let var = self.expect_ident()?;
        self.expect_contextual("in")?;
        let range_start = self.parse_expr()?;
        self.expect(TokenKind::DotDot)?;
        let range_end = self.parse_expr()?;

        let mut body = Vec::new();
        while !self.check_end_for() {
            body.push(self.parse_thread_stmt()?);
        }
        self.expect(TokenKind::End)?;
        let end_span = self.expect(TokenKind::For)?.span;

        Ok(ThreadStmt::For {
            var,
            start: range_start,
            end: range_end,
            body,
            span: start.merge(end_span),
        })
    }

    fn check_end_for(&self) -> bool {
        self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos].kind == TokenKind::End
            && self.tokens[self.pos + 1].kind == TokenKind::For
    }

    /// Check for `end lock` (contextual — `lock` is an Ident)
    fn check_end_lock(&self) -> bool {
        self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos].kind == TokenKind::End
            && matches!(&self.tokens[self.pos + 1].kind, TokenKind::Ident(s) if s == "lock")
    }

    /// Parse `lock resource_name ... end lock resource_name`
    fn parse_thread_lock(&mut self) -> Result<ThreadStmt, CompileError> {
        let start = self.expect_contextual("lock")?.span;
        let resource = self.expect_ident()?;

        let mut body = Vec::new();
        while !self.check_end_lock() {
            body.push(self.parse_thread_stmt()?);
        }
        self.expect(TokenKind::End)?;
        self.expect_contextual("lock")?;
        let closing = self.expect_ident()?;
        if closing.name != resource.name {
            return Err(CompileError::mismatched_closing(
                &resource.name, &closing.name, closing.span,
            ));
        }

        Ok(ThreadStmt::Lock {
            resource,
            body,
            span: start.merge(closing.span),
        })
    }

    /// Parse `do ... until cond;` inside a thread block.
    /// Body contains comb/seq assigns held while waiting for the condition.
    fn parse_thread_do_until(&mut self) -> Result<ThreadStmt, CompileError> {
        let start = self.expect_contextual("do")?.span;

        let mut body = Vec::new();
        while !self.check_ident("until") {
            body.push(self.parse_thread_stmt()?);
        }
        self.expect_contextual("until")?;
        let cond = self.parse_expr()?;
        let semi_span = self.expect(TokenKind::Semi)?.span;

        Ok(ThreadStmt::DoUntil {
            body,
            cond,
            span: start.merge(semi_span),
        })
    }

    /// Parse `resource name : mutex<policy>;`
    fn parse_resource_decl(&mut self) -> Result<ResourceDecl, CompileError> {
        let start = self.expect_contextual("resource")?.span;
        let name = self.expect_ident()?;
        self.expect(TokenKind::Colon)?;

        // Parse `mutex<policy>`
        self.expect_contextual("mutex")?;
        self.expect(TokenKind::Lt)?;
        let policy_ident = self.expect_ident()?;
        let policy = match policy_ident.name.as_str() {
            "round_robin" => ArbiterPolicy::RoundRobin,
            "priority"    => ArbiterPolicy::Priority,
            "lru"         => ArbiterPolicy::Lru,
            other => return Err(CompileError::general(
                &format!("unknown mutex policy `{}`; expected round_robin, priority, or lru", other),
                policy_ident.span,
            )),
        };
        self.expect(TokenKind::Gt)?;
        let end_span = self.expect(TokenKind::Semi)?.span;

        Ok(ResourceDecl {
            name,
            policy,
            span: start.merge(end_span),
        })
    }

    fn parse_latch_block(&mut self) -> Result<LatchBlock, CompileError> {
        let start = self.expect(TokenKind::Latch)?.span;
        self.expect(TokenKind::On)?;
        let enable = self.expect_ident()?;

        let mut stmts = Vec::new();
        while !self.check_end_latch() {
            stmts.push(self.parse_reg_stmt()?);
        }
        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Latch)?;
        let end_span = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(start);
        Ok(LatchBlock {
            enable,
            stmts,
            span: start.merge(end_span),
        })
    }

    fn check_end_latch(&self) -> bool {
        self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos].kind == TokenKind::End
            && self.tokens[self.pos + 1].kind == TokenKind::Latch
    }

    /// Parse `log(Level, "TAG", "fmt", arg, ...) ;`
    /// or    `log file("path") (Level, "TAG", "fmt", arg, ...) ;`
    fn parse_log_stmt(&mut self) -> Result<LogStmt, CompileError> {
        let start = self.expect(TokenKind::Log)?.span;

        // Optional: file("path")
        let file = if matches!(self.peek_kind(), Some(TokenKind::Ident(ref s)) if s == "file") {
            self.advance();
            self.expect(TokenKind::LParen)?;
            let path = match self.peek_kind() {
                Some(TokenKind::StringLit(s)) => { let p = s.clone(); self.advance(); p }
                _ => return Err(CompileError::unexpected_token(
                    "file path string literal",
                    &self.peek_kind().map(|k| k.to_string()).unwrap_or("EOF".into()),
                    self.peek_span(),
                )),
            };
            self.expect(TokenKind::RParen)?;
            Some(path)
        } else {
            None
        };

        self.expect(TokenKind::LParen)?;

        // Verbosity level: PascalCase ident
        let level = match self.peek_kind() {
            Some(TokenKind::Ident(name)) => {
                let level = match name.as_str() {
                    "Always" => LogLevel::Always,
                    "Low"    => LogLevel::Low,
                    "Medium" => LogLevel::Medium,
                    "High"   => LogLevel::High,
                    "Full"   => LogLevel::Full,
                    "Debug"  => LogLevel::Debug,
                    other => return Err(CompileError::general(
                        &format!("unknown log level `{other}`; expected Always, Low, Medium, High, Full, or Debug"),
                        self.peek_span(),
                    )),
                };
                self.advance();
                level
            }
            _ => return Err(CompileError::unexpected_token(
                "log level (Always/Low/Medium/High/Full/Debug)",
                &self.peek_kind().map(|k| k.to_string()).unwrap_or("EOF".into()),
                self.peek_span(),
            )),
        };
        self.expect(TokenKind::Comma)?;

        // Tag string
        let tag = match self.peek_kind() {
            Some(TokenKind::StringLit(s)) => { let t = s.clone(); self.advance(); t }
            _ => return Err(CompileError::unexpected_token(
                "tag string literal",
                &self.peek_kind().map(|k| k.to_string()).unwrap_or("EOF".into()),
                self.peek_span(),
            )),
        };
        self.expect(TokenKind::Comma)?;

        // Format string
        let fmt = match self.peek_kind() {
            Some(TokenKind::StringLit(s)) => { let f = s.clone(); self.advance(); f }
            _ => return Err(CompileError::unexpected_token(
                "format string literal",
                &self.peek_kind().map(|k| k.to_string()).unwrap_or("EOF".into()),
                self.peek_span(),
            )),
        };

        // Optional args
        let mut args = Vec::new();
        while self.eat(TokenKind::Comma) {
            args.push(self.parse_expr()?);
        }

        let end = self.expect(TokenKind::RParen)?.span;
        self.expect(TokenKind::Semi)?;
        Ok(LogStmt { level, tag, fmt, args, file, span: start.merge(end) })
    }

    fn parse_reg_stmt(&mut self) -> Result<Stmt, CompileError> {
        let unique = self.eat(TokenKind::Unique);
        if self.check(TokenKind::If) {
            return self.parse_reg_if(unique);
        }
        if self.check(TokenKind::Match) {
            return self.parse_reg_match(unique);
        }
        if unique {
            return Err(CompileError::general(
                "'unique' can only precede 'if' or 'match'",
                self.peek_span(),
            ));
        }
        if self.check(TokenKind::Log) {
            return Ok(Stmt::Log(self.parse_log_stmt()?));
        }
        if self.check(TokenKind::For) {
            return self.parse_for_loop(true);
        }
        if self.check(TokenKind::Init) {
            return self.parse_init_block();
        }
        // `wait until cond;` — pipeline stage multi-cycle boundary
        if self.check_ident("wait") {
            let wait_start = self.advance().span;
            if self.check_ident("until") {
                self.advance();
                let cond = self.parse_expr()?;
                let semi_span = self.expect(TokenKind::Semi)?.span;
                return Ok(Stmt::WaitUntil(cond, wait_start.merge(semi_span)));
            }
            return Err(CompileError::general(
                "expected 'until' after 'wait' in seq block",
                self.peek_span(),
            ));
        }
        // `do ... until cond;` — hold outputs while waiting for condition
        if self.check_ident("do") {
            let do_start = self.advance().span;
            let mut body = Vec::new();
            while !self.check_ident("until") {
                body.push(self.parse_reg_stmt()?);
            }
            self.advance(); // consume 'until'
            let cond = self.parse_expr()?;
            let semi_span = self.expect(TokenKind::Semi)?.span;
            return Ok(Stmt::DoUntil { body, cond, span: do_start.merge(semi_span) });
        }
        // Assignment: target <= value;
        let target = self.parse_expr()?;
        self.expect(TokenKind::LtEq)?;
        let value = self.parse_expr()?;
        self.expect(TokenKind::Semi)?;
        let span = target.span.merge(value.span);
        Ok(Stmt::Assign(RegAssign { target, value, span }))
    }

    fn parse_reg_if(&mut self, unique: bool) -> Result<Stmt, CompileError> {
        let start = self.expect(TokenKind::If)?.span;
        let cond = self.parse_expr()?;
        let mut then_stmts = Vec::new();
        while !self.check_end_if() && !self.check(TokenKind::Else) && !self.check(TokenKind::ElsIf) {
            then_stmts.push(self.parse_reg_stmt()?);
        }

        let mut else_stmts = Vec::new();
        if self.check(TokenKind::ElsIf) {
            // `elsif` — desugar to nested IfElse (replaces old `else if` chaining)
            // Rewrite the ElsIf token to If so parse_reg_if can consume it
            self.tokens[self.pos].kind = TokenKind::If;
            let nested = self.parse_reg_if(false)?;
            else_stmts.push(nested);
        } else if self.check(TokenKind::Else) {
            self.advance(); // consume `else`
            // `else` body — parse until `end if`
            while !self.check_end_if() {
                else_stmts.push(self.parse_reg_stmt()?);
            }
            self.expect(TokenKind::End)?;
            self.expect(TokenKind::If)?;
        } else {
            // No else branch — just `end if`
            self.expect(TokenKind::End)?;
            self.expect(TokenKind::If)?;
        }

        let end_span = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(start);
        Ok(Stmt::IfElse(IfElse {
            cond,
            then_stmts,
            else_stmts,
            unique,
            span: start.merge(end_span),
        }))
    }

    fn check_end_if(&self) -> bool {
        self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos].kind == TokenKind::End
            && self.tokens[self.pos + 1].kind == TokenKind::If
    }


    fn parse_reg_match(&mut self, unique: bool) -> Result<Stmt, CompileError> {
        let start = self.expect(TokenKind::Match)?.span;
        let scrutinee = self.parse_expr()?;
        let mut arms = Vec::new();
        while !self.check_end_match() {
            let pattern = self.parse_pattern()?;
            self.expect(TokenKind::FatArrow)?;
            let mut body = Vec::new();
            // Single statement or block until next pattern/end
            body.push(self.parse_reg_stmt()?);
            arms.push(MatchArm { pattern, body });
        }
        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Match)?;
        let end_span = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(start);
        Ok(Stmt::Match(MatchStmt {
            scrutinee,
            arms,
            unique,
            span: start.merge(end_span),
        }))
    }

    fn check_end_match(&self) -> bool {
        self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos].kind == TokenKind::End
            && self.tokens[self.pos + 1].kind == TokenKind::Match
    }

    fn parse_pattern(&mut self) -> Result<Pattern, CompileError> {
        if self.eat(TokenKind::Underscore) {
            return Ok(Pattern::Wildcard);
        }
        // Numeric literal patterns: `0 =>`, `42 =>`, `0xFF =>`
        match self.peek_kind() {
            Some(TokenKind::DecLiteral(_))
            | Some(TokenKind::HexLiteral(_))
            | Some(TokenKind::BinLiteral(_))
            | Some(TokenKind::SizedLiteral(_)) => {
                let expr = self.parse_literal()?;
                return Ok(Pattern::Literal(expr));
            }
            _ => {}
        }
        let ident = self.expect_ident()?;
        // `_` as identifier (alternative wildcard)
        if ident.name == "_" {
            return Ok(Pattern::Wildcard);
        }
        if self.eat(TokenKind::ColonColon) {
            let variant = self.expect_ident()?;
            Ok(Pattern::EnumVariant(ident, variant))
        } else {
            Ok(Pattern::Ident(ident))
        }
    }

    fn parse_comb_block(&mut self) -> Result<CombBlock, CompileError> {
        let start = self.expect(TokenKind::Comb)?.span;

        let mut stmts = Vec::new();
        while !self.check_end_comb() {
            stmts.push(self.parse_comb_stmt()?);
        }
        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Comb)?;
        let end_span = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(start);
        Ok(CombBlock {
            stmts,
            span: start.merge(end_span),
        })
    }


    fn check_end_comb(&self) -> bool {
        self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos].kind == TokenKind::End
            && self.tokens[self.pos + 1].kind == TokenKind::Comb
    }

    /// Convert a CombStmt to a Stmt for use in for-loop bodies.
    fn comb_stmt_to_stmt(cs: CombStmt) -> Stmt {
        match cs {
            CombStmt::Assign(a) => Stmt::Assign(RegAssign {
                target: a.target,
                value: a.value,
                span: a.span,
            }),
            CombStmt::IfElse(ie) => Stmt::IfElse(IfElse {
                cond: ie.cond,
                then_stmts: ie.then_stmts.into_iter().map(Self::comb_stmt_to_stmt).collect(),
                else_stmts: ie.else_stmts.into_iter().map(Self::comb_stmt_to_stmt).collect(),
                unique: ie.unique,
                span: ie.span,
            }),
            CombStmt::Log(l) => Stmt::Log(l),
            CombStmt::For(f) => Stmt::For(f),
            CombStmt::MatchExpr(m) => Stmt::Match(MatchStmt {
                scrutinee: m.scrutinee,
                arms: m.arms,
                unique: m.unique,
                span: m.span,
            }),
        }
    }

    /// Parse `for VAR in START..END ... end for`
    /// `is_seq`: true = body uses `parse_reg_stmt`, false = body uses `parse_comb_stmt` (wrapped)
    fn parse_for_loop(&mut self, is_seq: bool) -> Result<Stmt, CompileError> {
        let start = self.expect(TokenKind::For)?.span;
        let var = self.expect_ident()?;
        self.expect_contextual("in")?;

        let range = if self.check(TokenKind::LBrace) {
            self.advance();
            let mut values = Vec::new();
            loop {
                values.push(self.parse_expr()?);
                if !self.eat(TokenKind::Comma) { break; }
            }
            self.expect(TokenKind::RBrace)?;
            ForRange::ValueList(values)
        } else {
            let range_start = self.parse_expr()?;
            self.expect(TokenKind::DotDot)?;
            let range_end = self.parse_expr()?;
            ForRange::Range(range_start, range_end)
        };

        let mut body = Vec::new();
        while !(self.check(TokenKind::End)
            && self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos + 1].kind == TokenKind::For) {
            if is_seq {
                body.push(self.parse_reg_stmt()?);
            } else {
                // Wrap CombStmt into Stmt for unified ForLoop body
                let cs = self.parse_comb_stmt()?;
                body.push(Self::comb_stmt_to_stmt(cs));
            }
        }
        self.expect(TokenKind::End)?;
        let end_span = self.expect(TokenKind::For)?.span;
        Ok(Stmt::For(ForLoop {
            var,
            range,
            body,
            span: start.merge(end_span),
        }))
    }

    /// Parse `init on RST.asserted \n body \n end init`
    fn parse_init_block(&mut self) -> Result<Stmt, CompileError> {
        let start = self.expect(TokenKind::Init)?.span;
        self.expect(TokenKind::On)?;
        // Expect: IDENT.asserted
        let reset_signal = self.expect_ident()?;
        self.expect(TokenKind::Dot)?;
        let field = self.expect_ident()?;
        if field.name != "asserted" {
            return Err(CompileError::general(
                "expected `.asserted` after reset signal in `init on`",
                field.span,
            ));
        }
        let mut body = Vec::new();
        while !(self.check(TokenKind::End) && self.peek_kind_at(self.pos + 1) == Some(TokenKind::Init)) {
            body.push(self.parse_reg_stmt()?);
        }
        self.expect(TokenKind::End)?;
        let end_span = self.expect(TokenKind::Init)?.span;
        Ok(Stmt::Init(InitBlock {
            reset_signal,
            body,
            span: start.merge(end_span),
        }))
    }

    fn parse_comb_stmt(&mut self) -> Result<CombStmt, CompileError> {
        let unique = self.eat(TokenKind::Unique);
        if self.check(TokenKind::If) {
            return self.parse_comb_if(unique);
        }
        if self.check(TokenKind::Match) {
            return self.parse_comb_match(unique);
        }
        if unique {
            return Err(CompileError::general(
                "'unique' can only precede 'if' or 'match'",
                self.peek_span(),
            ));
        }
        if self.check(TokenKind::Log) {
            return Ok(CombStmt::Log(self.parse_log_stmt()?));
        }
        if self.check(TokenKind::For) {
            let fl = self.parse_for_loop(false)?;
            if let Stmt::For(f) = fl {
                return Ok(CombStmt::For(f));
            }
            unreachable!();
        }
        // target = expr;
        let target = self.parse_expr()?;
        self.expect(TokenKind::Eq)?;
        let value = self.parse_expr()?;
        self.expect(TokenKind::Semi)?;
        let span = target.span.merge(value.span);
        Ok(CombStmt::Assign(CombAssign {
            target,
            value,
            span,
        }))
    }

    fn parse_comb_if(&mut self, unique: bool) -> Result<CombStmt, CompileError> {
        let start = self.expect(TokenKind::If)?.span;
        let cond = self.parse_expr()?;
        let mut then_stmts = Vec::new();
        while !self.check_end_if() && !self.check(TokenKind::Else) && !self.check(TokenKind::ElsIf) {
            then_stmts.push(self.parse_comb_stmt()?);
        }

        let mut else_stmts = Vec::new();
        if self.check(TokenKind::ElsIf) {
            self.tokens[self.pos].kind = TokenKind::If;
            let nested = self.parse_comb_if(false)?;
            else_stmts.push(nested);
        } else if self.check(TokenKind::Else) {
            self.advance();
            while !self.check_end_if() {
                else_stmts.push(self.parse_comb_stmt()?);
            }
            self.expect(TokenKind::End)?;
            self.expect(TokenKind::If)?;
        } else {
            self.expect(TokenKind::End)?;
            self.expect(TokenKind::If)?;
        }

        let end_span = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(start);
        Ok(CombStmt::IfElse(CombIfElse {
            cond,
            then_stmts,
            else_stmts,
            unique,
            span: start.merge(end_span),
        }))
    }

    fn parse_comb_match(&mut self, unique: bool) -> Result<CombStmt, CompileError> {
        let start = self.expect(TokenKind::Match)?.span;
        let scrutinee = self.parse_expr()?;
        let mut arms = Vec::new();
        while !self.check_end_match() {
            let pattern = self.parse_pattern()?;
            self.expect(TokenKind::FatArrow)?;
            // Parse comb-style statements (with =) and convert to Stmt for MatchArm
            let comb_stmt = self.parse_comb_stmt()?;
            arms.push(MatchArm {
                pattern,
                body: vec![Self::comb_stmt_to_stmt(comb_stmt)],
            });
        }
        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Match)?;
        let end_span = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(start);
        Ok(CombStmt::MatchExpr(CombMatch {
            scrutinee,
            arms,
            unique,
            span: start.merge(end_span),
        }))
    }

    fn parse_let_binding(&mut self) -> Result<LetBinding, CompileError> {
        let start = self.expect(TokenKind::Let)?.span;
        let name = self.expect_ident()?;
        let ty = if self.eat(TokenKind::Colon) {
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        self.expect(TokenKind::Eq)?;
        let value = self.parse_expr()?;
        self.expect(TokenKind::Semi)?;
        let end_span = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(start);
        Ok(LetBinding {
            name,
            ty,
            value,
            span: start.merge(end_span),
        })
    }

    fn parse_pipe_reg_decl(&mut self) -> Result<PipeRegDecl, CompileError> {
        let start = self.expect(TokenKind::PipeReg)?.span;
        let name = self.expect_ident()?;
        self.expect(TokenKind::Colon)?;
        let source = self.expect_ident()?;
        // Expect the contextual keyword "stages"
        let stages_ident = self.expect_ident()?;
        if stages_ident.name != "stages" {
            return Err(CompileError::unexpected_token(
                "stages", &stages_ident.name, stages_ident.span,
            ));
        }
        let stages_tok = self.advance();
        let stages = match &stages_tok.kind {
            TokenKind::DecLiteral(s) => s.parse::<u32>().map_err(|_|
                CompileError::general("invalid stage count", stages_tok.span))?,
            _ => return Err(CompileError::unexpected_token(
                "integer literal", &stages_tok.kind.to_string(), stages_tok.span)),
        };
        self.expect(TokenKind::Semi)?;
        let end_span = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(start);
        Ok(PipeRegDecl { name, source, stages, span: start.merge(end_span) })
    }

    // ── Assert / Cover ────────────────────────────────────────────────────────

    /// Parse `assert [name:] expr;` or `cover [name:] expr;`
    ///
    /// Name disambiguation: if the next token is an Ident followed by `:` (and
    /// the token after `:` is NOT `:`, ruling out `::` paths), treat it as the
    /// label. Otherwise the expression starts immediately.
    fn parse_assert_decl(&mut self) -> Result<AssertDecl, CompileError> {
        let start = self.peek_span();
        let kind = match self.peek_kind() {
            Some(TokenKind::Assert) => { self.advance(); AssertKind::Assert }
            Some(TokenKind::Cover)  => { self.advance(); AssertKind::Cover  }
            _ => return Err(CompileError::general("expected assert or cover", self.peek_span())),
        };

        // Optional label: `name :` where `:` is not followed by another `:`
        let name = if matches!(self.peek_kind(), Some(TokenKind::Ident(_)))
            && self.peek_kind_at(self.pos + 1) == Some(TokenKind::Colon)
            && self.peek_kind_at(self.pos + 2) != Some(TokenKind::Colon)
        {
            let n = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            Some(n)
        } else {
            None
        };

        let expr = self.parse_expr()?;
        let end = self.expect(TokenKind::Semi)?.span;
        Ok(AssertDecl { kind, name, expr, span: start.merge(end) })
    }

    fn parse_inst(&mut self) -> Result<InstDecl, CompileError> {
        let start = self.expect(TokenKind::Inst)?.span;
        let name = self.expect_ident()?;
        self.expect(TokenKind::Colon)?;
        let module_name = self.expect_ident()?;

        let mut param_assigns = Vec::new();
        let mut connections = Vec::new();

        while !self.check_end_inst() {
            if self.check_param() {
                self.advance();
                let pname = self.expect_ident()?;
                self.expect(TokenKind::Eq)?;
                let value = self.parse_expr()?;
                self.expect(TokenKind::Semi)?;
                param_assigns.push(ParamAssign { name: pname, value });
            } else if matches!(self.peek_kind(), Some(TokenKind::Ident(_))) {
                let cstart = self.peek_span();
                let mut port_name = self.expect_ident()?;
                // Support indexed port group syntax: name[i].member → namei_member
                if self.eat(TokenKind::LBracket) {
                    let idx_tok = self.advance();
                    let idx = match &idx_tok.kind {
                        TokenKind::DecLiteral(s) => s.parse::<u32>().map_err(|_|
                            CompileError::general("invalid port index", idx_tok.span))?,
                        _ => return Err(CompileError::unexpected_token(
                            "integer index", &idx_tok.kind.to_string(), idx_tok.span)),
                    };
                    self.expect(TokenKind::RBracket)?;
                    self.expect(TokenKind::Dot)?;
                    let member = self.expect_ident()?;
                    port_name = Ident::new(
                        format!("{}{idx}_{}", port_name.name, member.name),
                        port_name.span.merge(member.span),
                    );
                // Support dot notation for port group members: group.member → group_member
                } else if self.eat(TokenKind::Dot) {
                    let member = self.expect_ident()?;
                    port_name = Ident::new(
                        format!("{}_{}", port_name.name, member.name),
                        port_name.span.merge(member.span),
                    );
                }
                let direction = if self.eat(TokenKind::LArrow) {
                    ConnectDir::Input
                } else if self.eat(TokenKind::RArrow) {
                    ConnectDir::Output
                } else {
                    return Err(CompileError::unexpected_token(
                        "<- or ->",
                        &self.peek_kind().map(|k| k.to_string()).unwrap_or("EOF".into()),
                        self.peek_span(),
                    ));
                };
                let signal = self.parse_expr()?;
                // Optional: `as Reset<Async[, Low]>` — override reset type at instantiation
                let reset_override = if self.check(TokenKind::As) {
                    if matches!(self.peek_kind_at(self.pos + 1), Some(TokenKind::Reset)) {
                        self.advance(); // consume `as`
                        self.advance(); // consume `Reset`
                        self.expect(TokenKind::Lt)?;
                        let kind = if self.eat(TokenKind::Sync) {
                            ResetKind::Sync
                        } else if self.eat(TokenKind::Async) {
                            ResetKind::Async
                        } else {
                            return Err(CompileError::unexpected_token(
                                "Sync or Async",
                                &self.peek_kind().map(|k| k.to_string()).unwrap_or("EOF".into()),
                                self.peek_span(),
                            ));
                        };
                        let level = if self.eat(TokenKind::Comma) {
                            match self.peek_kind() {
                                Some(TokenKind::Ident(s)) if s == "High" => { self.advance(); ResetLevel::High }
                                Some(TokenKind::Ident(s)) if s == "Low"  => { self.advance(); ResetLevel::Low  }
                                _ => return Err(CompileError::unexpected_token(
                                    "High or Low",
                                    &self.peek_kind().map(|k| k.to_string()).unwrap_or("EOF".into()),
                                    self.peek_span(),
                                )),
                            }
                        } else {
                            ResetLevel::High
                        };
                        self.expect(TokenKind::Gt)?;
                        Some((kind, level))
                    } else {
                        None
                    }
                } else {
                    None
                };
                self.expect(TokenKind::Semi)?;
                let end_span = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(cstart);
                connections.push(Connection {
                    port_name,
                    direction,
                    signal,
                    reset_override,
                    span: cstart.merge(end_span),
                });
            } else {
                return Err(CompileError::unexpected_token(
                    "param or port connection",
                    &self.peek_kind().map(|k| k.to_string()).unwrap_or("EOF".into()),
                    self.peek_span(),
                ));
            }
        }

        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Inst)?;
        let closing_name = self.expect_ident()?;
        if closing_name.name != name.name {
            return Err(CompileError::mismatched_closing(
                &name.name,
                &closing_name.name,
                closing_name.span,
            ));
        }

        Ok(InstDecl {
            span: start.merge(closing_name.span),
            name,
            module_name,
            param_assigns,
            connections,
        })
    }

    fn check_end_inst(&self) -> bool {
        self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos].kind == TokenKind::End
            && self.tokens[self.pos + 1].kind == TokenKind::Inst
    }

    // ── Generate ──────────────────────────────────────────────────────────────

    fn check_end_generate_for(&self) -> bool {
        self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos].kind == TokenKind::End
            && self.tokens[self.pos + 1].kind == TokenKind::GenerateFor
    }

    fn check_end_generate_if(&self) -> bool {
        // Stop at `end generate_if`
        if self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos].kind == TokenKind::End
            && self.tokens[self.pos + 1].kind == TokenKind::GenerateIf
        { return true; }
        // Also stop at `generate_else` so the caller can consume it
        if self.check(TokenKind::GenerateElse) { return true; }
        false
    }

    fn parse_gen_items_for(&mut self) -> Result<Vec<GenItem>, CompileError> {
        let mut items = Vec::new();
        while !self.check_end_generate_for() {
            match self.peek_kind() {
                Some(TokenKind::Port) => items.push(GenItem::Port(self.parse_port_decl()?)),
                Some(TokenKind::Inst) => items.push(GenItem::Inst(self.parse_inst()?)),
                Some(TokenKind::Thread) => items.push(GenItem::Thread(self.parse_thread_block()?)),
                Some(TokenKind::Assert) | Some(TokenKind::Cover) => {
                    items.push(GenItem::Assert(self.parse_assert_decl()?));
                }
                Some(other) => {
                    return Err(CompileError::unexpected_token(
                        "port, inst, thread, assert, or cover",
                        &other.to_string(),
                        self.peek_span(),
                    ));
                }
                None => return Err(CompileError::UnexpectedEof),
            }
        }
        Ok(items)
    }

    fn parse_gen_items_if(&mut self) -> Result<Vec<GenItem>, CompileError> {
        let mut items = Vec::new();
        while !self.check_end_generate_if() {
            match self.peek_kind() {
                Some(TokenKind::Port) => items.push(GenItem::Port(self.parse_port_decl()?)),
                Some(TokenKind::Inst) => items.push(GenItem::Inst(self.parse_inst()?)),
                Some(TokenKind::Thread) => items.push(GenItem::Thread(self.parse_thread_block()?)),
                Some(TokenKind::Assert) | Some(TokenKind::Cover) => {
                    items.push(GenItem::Assert(self.parse_assert_decl()?));
                }
                Some(other) => {
                    return Err(CompileError::unexpected_token(
                        "port, inst, thread, assert, or cover",
                        &other.to_string(),
                        self.peek_span(),
                    ));
                }
                None => return Err(CompileError::UnexpectedEof),
            }
        }
        Ok(items)
    }

    fn parse_generate_for(&mut self) -> Result<GenerateDecl, CompileError> {
        let start = self.expect(TokenKind::GenerateFor)?.span;
        let var = self.expect_ident()?;
        self.expect_contextual("in")?;
        let range_start = self.parse_expr()?;
        self.expect(TokenKind::DotDot)?;
        let range_end = self.parse_expr()?;
        let items = self.parse_gen_items_for()?;
        // Consume `end generate_for`
        self.expect(TokenKind::End)?;
        let end_span = self.expect(TokenKind::GenerateFor)?.span;
        Ok(GenerateDecl::For(GenerateFor {
            span: start.merge(end_span),
            var,
            start: range_start,
            end: range_end,
            items,
        }))
    }

    fn parse_generate_if(&mut self) -> Result<GenerateDecl, CompileError> {
        let start = self.expect(TokenKind::GenerateIf)?.span;
        let cond = self.parse_expr()?;
        let then_items = self.parse_gen_items_if()?;
        // Optional `generate_else ... end generate_if`
        let else_items = if self.check(TokenKind::GenerateElse) {
            self.advance(); // consume `generate_else`
            self.parse_gen_items_if()?
        } else {
            Vec::new()
        };
        // Consume `end generate_if`
        self.expect(TokenKind::End)?;
        let end_span = self.expect(TokenKind::GenerateIf)?.span;
        Ok(GenerateDecl::If(GenerateIf {
            span: start.merge(end_span),
            cond,
            then_items,
            else_items,
        }))
    }


    /// Parse an expression inside angle brackets (no `>` or `>=` as binop)
    fn parse_type_arg_expr(&mut self) -> Result<Expr, CompileError> {
        let old = self.no_angle;
        self.no_angle = true;
        let result = self.parse_expr();
        self.no_angle = old;
        result
    }

    // --- Type Expressions ---
    pub fn parse_type_expr(&mut self) -> Result<TypeExpr, CompileError> {
        match self.peek_kind() {
            Some(TokenKind::UInt) => {
                self.advance();
                self.expect(TokenKind::Lt)?;
                let width = self.parse_type_arg_expr()?;
                self.expect(TokenKind::Gt)?;
                Ok(TypeExpr::UInt(Box::new(width)))
            }
            Some(TokenKind::SInt) => {
                self.advance();
                self.expect(TokenKind::Lt)?;
                let width = self.parse_type_arg_expr()?;
                self.expect(TokenKind::Gt)?;
                Ok(TypeExpr::SInt(Box::new(width)))
            }
            Some(TokenKind::Bool) => {
                self.advance();
                Ok(TypeExpr::Bool)
            }
            Some(TokenKind::Bit) => {
                self.advance();
                Ok(TypeExpr::Bit)
            }
            Some(TokenKind::Clock) => {
                self.advance();
                self.expect(TokenKind::Lt)?;
                let domain = self.expect_ident()?;
                self.expect(TokenKind::Gt)?;
                Ok(TypeExpr::Clock(domain))
            }
            Some(TokenKind::Reset) => {
                self.advance();
                self.expect(TokenKind::Lt)?;
                // kind: Sync | Async
                let kind = if self.eat(TokenKind::Sync) {
                    ResetKind::Sync
                } else if self.eat(TokenKind::Async) {
                    ResetKind::Async
                } else {
                    return Err(CompileError::unexpected_token(
                        "Sync or Async",
                        &self.peek_kind().map(|k| k.to_string()).unwrap_or("EOF".into()),
                        self.peek_span(),
                    ));
                };
                // Optional polarity: Reset<Sync, High> or Reset<Sync> (defaults High)
                let level = if self.eat(TokenKind::Comma) {
                    match self.peek_kind() {
                        Some(TokenKind::Ident(s)) if s == "High" => {
                            self.advance();
                            ResetLevel::High
                        }
                        Some(TokenKind::Ident(s)) if s == "Low" => {
                            self.advance();
                            ResetLevel::Low
                        }
                        _ => {
                            return Err(CompileError::unexpected_token(
                                "High or Low",
                                &self.peek_kind().map(|k| k.to_string()).unwrap_or("EOF".into()),
                                self.peek_span(),
                            ));
                        }
                    }
                } else {
                    ResetLevel::High // default
                };
                self.expect(TokenKind::Gt)?;
                Ok(TypeExpr::Reset(kind, level))
            }
            Some(TokenKind::KwVec) => {
                self.advance();
                self.expect(TokenKind::Lt)?;
                let elem = self.parse_type_expr()?;
                self.expect(TokenKind::Comma)?;
                let size = self.parse_type_arg_expr()?;
                self.expect(TokenKind::Gt)?;
                Ok(TypeExpr::Vec(Box::new(elem), Box::new(size)))
            }
            Some(TokenKind::Ident(_)) => {
                let ident = self.expect_ident()?;
                Ok(TypeExpr::Named(ident))
            }
            Some(other) => Err(CompileError::unexpected_token(
                "type",
                &other.to_string(),
                self.peek_span(),
            )),
            None => Err(CompileError::UnexpectedEof),
        }
    }

    // --- Expression Parsing (Pratt) ---
    pub fn parse_expr(&mut self) -> Result<Expr, CompileError> {
        self.parse_ternary()
    }

    /// Parse a ternary expression (lower precedence than all binary ops).
    /// Right-associative: a ? b : c ? d : e  →  a ? b : (c ? d : e)
    fn parse_ternary(&mut self) -> Result<Expr, CompileError> {
        let cond = self.parse_expr_bp(0)?;
        if self.check(TokenKind::Question) {
            self.advance(); // consume `?`
            let then_expr = self.parse_ternary()?;
            self.expect(TokenKind::Colon)?;
            let else_expr = self.parse_ternary()?;
            let span = cond.span.merge(else_expr.span);
            Ok(Expr {
                kind: ExprKind::Ternary(Box::new(cond), Box::new(then_expr), Box::new(else_expr)),
                span, parenthesized: false })
        } else {
            Ok(cond)
        }
    }

    fn parse_expr_bp(&mut self, min_bp: u8) -> Result<Expr, CompileError> {
        let mut lhs = self.parse_prefix()?;

        loop {
            // Postfix: `.field`, `.method<N>()`, `[i]`, `as T`
            if self.check(TokenKind::Dot) {
                self.advance();
                let field = self.expect_ident()?;
                // Check for method call: .trunc<N>(), .zext<N>(), .sext<N>(), .reverse(N)
                if self.check(TokenKind::LParen) && field.name == "reverse" {
                    self.advance(); // (
                    let mut args = vec![self.parse_expr()?];
                    while self.check(TokenKind::Comma) {
                        self.advance();
                        args.push(self.parse_expr()?);
                    }
                    self.expect(TokenKind::RParen)?;
                    let span = lhs.span.merge(self.tokens[self.pos.saturating_sub(1)].span);
                    lhs = Expr {
                        kind: ExprKind::MethodCall(Box::new(lhs), field, args),
                        span, parenthesized: false };
                } else if self.check(TokenKind::Lt) && is_method_name(&field.name) {
                    self.advance(); // <
                    let old_no_angle = self.no_angle;
                    self.no_angle = true;
                    let mut type_args = vec![self.parse_expr()?];
                    while self.check(TokenKind::Comma) {
                        self.advance();
                        type_args.push(self.parse_expr()?);
                    }
                    self.no_angle = old_no_angle;
                    self.expect(TokenKind::Gt)?;
                    self.expect(TokenKind::LParen)?;
                    self.expect(TokenKind::RParen)?;
                    let span = lhs.span.merge(self.tokens[self.pos.saturating_sub(1)].span);
                    lhs = Expr {
                        kind: ExprKind::MethodCall(Box::new(lhs), field, type_args),
                        span, parenthesized: false };
                } else {
                    let span = lhs.span.merge(field.span);
                    lhs = Expr {
                        kind: ExprKind::FieldAccess(Box::new(lhs), field),
                        span, parenthesized: false };
                }
                continue;
            }

            if self.check(TokenKind::LBracket) {
                self.advance();
                let first = self.parse_expr()?;
                // Part-select: expr[start +: width] or expr[start -: width]
                // `+:` / `-:` may arrive as a single PlusColon/MinusColon token (no space)
                // OR as a separate Plus/Minus + Colon token pair (with space); handle both.
                let is_plus_colon  = self.check(TokenKind::PlusColon)
                    || (self.check(TokenKind::Plus)  && self.peek_kind_at(self.pos + 1) == Some(TokenKind::Colon));
                let is_minus_colon = self.check(TokenKind::MinusColon)
                    || (self.check(TokenKind::Minus) && self.peek_kind_at(self.pos + 1) == Some(TokenKind::Colon));
                if is_plus_colon || is_minus_colon {
                    let up = is_plus_colon;
                    self.advance(); // consume + or - (or +: as one token)
                    // If spaced form, also consume the separate `:` token
                    if self.check(TokenKind::Colon) { self.advance(); }
                    let width = self.parse_expr()?;
                    self.expect(TokenKind::RBracket)?;
                    let span = lhs.span.merge(self.tokens[self.pos.saturating_sub(1)].span);
                    lhs = Expr {
                        kind: ExprKind::PartSelect(Box::new(lhs), Box::new(first), Box::new(width), up),
                        span, parenthesized: false };
                } else if self.check(TokenKind::Colon) {
                    // bit-slice: expr[hi:lo]
                    self.advance();
                    let lo = self.parse_expr()?;
                    self.expect(TokenKind::RBracket)?;
                    let span = lhs.span.merge(self.tokens[self.pos.saturating_sub(1)].span);
                    lhs = Expr {
                        kind: ExprKind::BitSlice(Box::new(lhs), Box::new(first), Box::new(lo)),
                        span, parenthesized: false };
                } else {
                    // index: expr[i]
                    self.expect(TokenKind::RBracket)?;
                    let span = lhs.span.merge(self.tokens[self.pos.saturating_sub(1)].span);
                    lhs = Expr {
                        kind: ExprKind::Index(Box::new(lhs), Box::new(first)),
                        span, parenthesized: false };
                }
                continue;
            }

            if self.check(TokenKind::As) {
                self.advance();
                let ty = self.parse_type_expr()?;
                let span = lhs.span; // approximate
                lhs = Expr {
                    kind: ExprKind::Cast(Box::new(lhs), Box::new(ty)),
                    span, parenthesized: false };
                continue;
            }

            // `inside` set membership operator
            if self.check(TokenKind::Inside) {
                let lhs_span = lhs.span;
                self.advance();
                self.expect(TokenKind::LBrace)?;
                let mut members = Vec::new();
                loop {
                    let e = self.parse_expr()?;
                    if self.eat(TokenKind::DotDot) {
                        let end = self.parse_expr()?;
                        members.push(InsideMember::Range(e, end));
                    } else {
                        members.push(InsideMember::Single(e));
                    }
                    if !self.eat(TokenKind::Comma) { break; }
                }
                let end_span = self.expect(TokenKind::RBrace)?.span;
                lhs = Expr { kind: ExprKind::Inside(Box::new(lhs), members), span: lhs_span.merge(end_span), parenthesized: false };
                continue;
            }

            // Infix
            let Some(op) = self.peek_binop() else { break };
            let (l_bp, r_bp) = infix_binding_power(op);
            if l_bp < min_bp {
                break;
            }
            self.advance(); // consume operator (first token)
            // Wrapping operators are two tokens (+%, -%, *%); consume the trailing %
            if matches!(op, BinOp::AddWrap | BinOp::SubWrap | BinOp::MulWrap) {
                self.advance(); // consume %
            }
            let rhs = self.parse_expr_bp(r_bp)?;
            let span = lhs.span.merge(rhs.span);
            lhs = Expr {
                kind: ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)),
                span, parenthesized: false };
        }

        Ok(lhs)
    }

    fn parse_prefix(&mut self) -> Result<Expr, CompileError> {
        match self.peek_kind() {
            Some(TokenKind::Not) => {
                let tok = self.advance();
                let operand = self.parse_expr_bp(prefix_bp())?;
                let span = tok.span.merge(operand.span);
                Ok(Expr {
                    kind: ExprKind::Unary(UnaryOp::Not, Box::new(operand)),
                    span, parenthesized: false })
            }
            Some(TokenKind::Tilde) => {
                let tok = self.advance();
                let operand = self.parse_expr_bp(prefix_bp())?;
                let span = tok.span.merge(operand.span);
                Ok(Expr {
                    kind: ExprKind::Unary(UnaryOp::BitNot, Box::new(operand)),
                    span, parenthesized: false })
            }
            Some(TokenKind::Minus) => {
                let tok = self.advance();
                let operand = self.parse_expr_bp(prefix_bp())?;
                let span = tok.span.merge(operand.span);
                Ok(Expr {
                    kind: ExprKind::Unary(UnaryOp::Neg, Box::new(operand)),
                    span, parenthesized: false })
            }
            Some(TokenKind::Amp) => {
                let tok = self.advance();
                let operand = self.parse_expr_bp(prefix_bp())?;
                let span = tok.span.merge(operand.span);
                Ok(Expr {
                    kind: ExprKind::Unary(UnaryOp::RedAnd, Box::new(operand)),
                    span, parenthesized: false })
            }
            Some(TokenKind::Pipe) => {
                let tok = self.advance();
                let operand = self.parse_expr_bp(prefix_bp())?;
                let span = tok.span.merge(operand.span);
                Ok(Expr {
                    kind: ExprKind::Unary(UnaryOp::RedOr, Box::new(operand)),
                    span, parenthesized: false })
            }
            Some(TokenKind::Caret) => {
                let tok = self.advance();
                let operand = self.parse_expr_bp(prefix_bp())?;
                let span = tok.span.merge(operand.span);
                Ok(Expr {
                    kind: ExprKind::Unary(UnaryOp::RedXor, Box::new(operand)),
                    span, parenthesized: false })
            }
            Some(TokenKind::LParen) => {
                self.advance();
                let mut expr = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
                expr.parenthesized = true;
                Ok(expr)
            }
            // $clog2(expr)
            Some(TokenKind::Clog2) => {
                let start = self.advance().span;
                self.expect(TokenKind::LParen)?;
                let arg = self.parse_expr()?;
                let end = self.expect(TokenKind::RParen)?;
                Ok(Expr {
                    kind: ExprKind::Clog2(Box::new(arg)),
                    span: start.merge(end.span), parenthesized: false })
            }
            // signed(expr) — same-width reinterpret to SInt
            Some(TokenKind::Signed) => {
                let start = self.advance().span;
                self.expect(TokenKind::LParen)?;
                let arg = self.parse_expr()?;
                let end = self.expect(TokenKind::RParen)?;
                Ok(Expr {
                    kind: ExprKind::Signed(Box::new(arg)),
                    span: start.merge(end.span), parenthesized: false })
            }
            // onehot(index) — one-hot decode: 1 << index
            Some(TokenKind::Onehot) => {
                let start = self.advance().span;
                self.expect(TokenKind::LParen)?;
                let arg = self.parse_expr()?;
                let end = self.expect(TokenKind::RParen)?;
                Ok(Expr {
                    kind: ExprKind::Onehot(Box::new(arg)),
                    span: start.merge(end.span), parenthesized: false })
            }
            // unsigned(expr) — same-width reinterpret to UInt
            Some(TokenKind::KwUnsigned) => {
                let start = self.advance().span;
                self.expect(TokenKind::LParen)?;
                let arg = self.parse_expr()?;
                let end = self.expect(TokenKind::RParen)?;
                Ok(Expr {
                    kind: ExprKind::Unsigned(Box::new(arg)),
                    span: start.merge(end.span), parenthesized: false })
            }
            // Bit concatenation {a, b, c} or bit replication {N{expr}}
            Some(TokenKind::LBrace) => {
                let start = self.advance().span;
                // Check for replication: {N{expr}} — count/ident followed by LBrace
                let is_repeat = if let Some(TokenKind::DecLiteral(_) | TokenKind::HexLiteral(_) | TokenKind::Ident(_)) = self.peek_kind() {
                    // Look ahead: if token after the number/ident is '{', it's replication
                    self.pos + 1 < self.tokens.len() && self.tokens[self.pos + 1].kind == TokenKind::LBrace
                } else {
                    false
                };
                if is_repeat {
                    let count = self.parse_expr()?;
                    self.expect(TokenKind::LBrace)?;
                    let value = self.parse_expr()?;
                    self.expect(TokenKind::RBrace)?;
                    let end = self.expect(TokenKind::RBrace)?;
                    Ok(Expr {
                        kind: ExprKind::Repeat(Box::new(count), Box::new(value)),
                        span: start.merge(end.span), parenthesized: false })
                } else {
                    let mut parts = Vec::new();
                    while !self.check(TokenKind::RBrace) {
                        parts.push(self.parse_expr()?);
                        if !self.eat(TokenKind::Comma) {
                            break;
                        }
                    }
                    let end = self.expect(TokenKind::RBrace)?;
                    Ok(Expr {
                        kind: ExprKind::Concat(parts),
                        span: start.merge(end.span), parenthesized: false })
                }
            }
            Some(TokenKind::Todo) => {
                let tok = self.advance();
                Ok(Expr {
                    kind: ExprKind::Todo,
                    span: tok.span, parenthesized: false })
            }
            Some(TokenKind::True) => {
                let tok = self.advance();
                Ok(Expr {
                    kind: ExprKind::Bool(true),
                    span: tok.span, parenthesized: false })
            }
            Some(TokenKind::False) => {
                let tok = self.advance();
                Ok(Expr {
                    kind: ExprKind::Bool(false),
                    span: tok.span, parenthesized: false })
            }
            Some(TokenKind::DecLiteral(_)) | Some(TokenKind::HexLiteral(_))
            | Some(TokenKind::BinLiteral(_)) | Some(TokenKind::SizedLiteral(_)) => {
                self.parse_literal()
            }
            Some(TokenKind::Ident(_)) => {
                let ident = self.expect_ident()?;
                // Check for enum variant: Ident::Ident
                if self.check(TokenKind::ColonColon) {
                    self.advance();
                    let variant = self.expect_ident()?;
                    let span = ident.span.merge(variant.span);
                    Ok(Expr {
                        kind: ExprKind::EnumVariant(ident, variant),
                        span, parenthesized: false })
                }
                // Check for struct literal: Ident { ... }
                else if self.check(TokenKind::LBrace) {
                    self.advance();
                    let mut fields = Vec::new();
                    while !self.check(TokenKind::RBrace) {
                        let fname = self.expect_ident()?;
                        self.expect(TokenKind::Colon)?;
                        let value = self.parse_expr()?;
                        fields.push(FieldInit { name: fname, value });
                        if !self.eat(TokenKind::Comma) {
                            break;
                        }
                    }
                    let end = self.expect(TokenKind::RBrace)?;
                    let span = ident.span.merge(end.span);
                    Ok(Expr {
                        kind: ExprKind::StructLiteral(ident, fields),
                        span, parenthesized: false })
                } else if self.check(TokenKind::LParen) {
                    // Function call: Name(arg, ...)
                    self.advance(); // consume `(`
                    let mut call_args = Vec::new();
                    while !self.check(TokenKind::RParen) {
                        call_args.push(self.parse_expr()?);
                        if !self.eat(TokenKind::Comma) {
                            break;
                        }
                    }
                    let end = self.expect(TokenKind::RParen)?;
                    let span = ident.span.merge(end.span);
                    Ok(Expr {
                        kind: ExprKind::FunctionCall(ident.name, call_args),
                        span, parenthesized: false })
                } else {
                    let span = ident.span;
                    Ok(Expr {
                        kind: ExprKind::Ident(ident.name),
                        span, parenthesized: false })
                }
            }
            // match expression: match scrutinee  pat => expr, ... end match
            Some(TokenKind::Match) => {
                let start = self.advance().span; // consume `match`
                let scrutinee = self.parse_expr()?;
                let mut arms: Vec<ExprMatchArm> = Vec::new();
                while !self.check_end_match() {
                    let pattern = self.parse_pattern()?;
                    self.expect(TokenKind::FatArrow)?;
                    let value = self.parse_expr()?;
                    self.eat(TokenKind::Comma);
                    arms.push(ExprMatchArm { pattern, value });
                }
                self.expect(TokenKind::End)?;
                self.expect(TokenKind::Match)?;
                let end_span = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(start);
                Ok(Expr {
                    kind: ExprKind::ExprMatch(Box::new(scrutinee), arms),
                    span: start.merge(end_span), parenthesized: false })
            }
            Some(other) => Err(CompileError::unexpected_token(
                "expression",
                &other.to_string(),
                self.peek_span(),
            )),
            None => Err(CompileError::UnexpectedEof),
        }
    }

    fn parse_literal(&mut self) -> Result<Expr, CompileError> {
        let tok = self.advance();
        let kind = match &tok.kind {
            TokenKind::DecLiteral(s) => {
                let v = s.replace('_', "").parse::<u64>().map_err(|_| {
                    CompileError::general("invalid decimal literal", tok.span)
                })?;
                ExprKind::Literal(LitKind::Dec(v))
            }
            TokenKind::HexLiteral(s) => {
                let v = u64::from_str_radix(&s[2..].replace('_', ""), 16).map_err(|_| {
                    CompileError::general("invalid hex literal", tok.span)
                })?;
                ExprKind::Literal(LitKind::Hex(v))
            }
            TokenKind::BinLiteral(s) => {
                let v = u64::from_str_radix(&s[2..].replace('_', ""), 2).map_err(|_| {
                    CompileError::general("invalid binary literal", tok.span)
                })?;
                ExprKind::Literal(LitKind::Bin(v))
            }
            TokenKind::SizedLiteral(s) => {
                // format: WIDTH'BASE_CHAR VALUE
                let parts: Vec<&str> = s.splitn(2, '\'').collect();
                let width: u32 = parts[0].parse().map_err(|_| {
                    CompileError::general("invalid sized literal width", tok.span)
                })?;
                let base_char = parts[1].chars().next().unwrap();
                let digits = &parts[1][1..].replace('_', "");
                let value = match base_char {
                    'h' | 'H' => u64::from_str_radix(digits, 16),
                    'b' | 'B' => u64::from_str_radix(digits, 2),
                    'd' | 'D' => digits.parse::<u64>(),
                    _ => return Err(CompileError::general("invalid sized literal base", tok.span)),
                }
                .map_err(|_| CompileError::general("invalid sized literal value", tok.span))?;
                ExprKind::Literal(LitKind::Sized(width, value))
            }
            _ => unreachable!(),
        };
        Ok(Expr {
            kind,
            span: tok.span, parenthesized: false })
    }

    fn peek_binop(&self) -> Option<BinOp> {
        match self.peek_kind()? {
            // Don't treat `+ :` or `- :` as binary ops — they are part-select separators
            TokenKind::Plus if self.peek_kind_at(self.pos + 1) == Some(TokenKind::Colon) => None,
            TokenKind::Minus if self.peek_kind_at(self.pos + 1) == Some(TokenKind::Colon) => None,
            TokenKind::Plus if self.peek_kind_at(self.pos + 1) == Some(TokenKind::Percent) => Some(BinOp::AddWrap),
            TokenKind::Minus if self.peek_kind_at(self.pos + 1) == Some(TokenKind::Percent) => Some(BinOp::SubWrap),
            TokenKind::Star if self.peek_kind_at(self.pos + 1) == Some(TokenKind::Percent) => Some(BinOp::MulWrap),
            TokenKind::Plus => Some(BinOp::Add),
            TokenKind::Minus => Some(BinOp::Sub),
            TokenKind::Star => Some(BinOp::Mul),
            TokenKind::Slash => Some(BinOp::Div),
            TokenKind::Percent => Some(BinOp::Mod),
            TokenKind::EqEq => Some(BinOp::Eq),
            TokenKind::BangEq => Some(BinOp::Neq),
            TokenKind::Lt if !self.no_angle => Some(BinOp::Lt),
            TokenKind::Gt if !self.no_angle => Some(BinOp::Gt),
            TokenKind::GtEq if !self.no_angle => Some(BinOp::Gte),
            TokenKind::And => Some(BinOp::And),
            TokenKind::Or => Some(BinOp::Or),
            TokenKind::Implies => Some(BinOp::Implies),
            TokenKind::Amp => Some(BinOp::BitAnd),
            TokenKind::Pipe => Some(BinOp::BitOr),
            TokenKind::Caret => Some(BinOp::BitXor),
            TokenKind::Shl => Some(BinOp::Shl),
            TokenKind::Shr => Some(BinOp::Shr),
            _ => None,
        }
    }

    // ── FSM ───────────────────────────────────────────────────────────────────

    fn parse_fsm(&mut self) -> Result<FsmDecl, CompileError> {
        let start = self.expect(TokenKind::Fsm)?.span;
        let name = self.expect_ident()?;
        self.seq_default = None;

        let mut params = Vec::new();
        let mut ports = Vec::new();
        let mut regs = Vec::new();
        let mut lets = Vec::new();
        let mut wires = Vec::new();
        let mut state_names: Vec<Ident> = Vec::new();
        let mut default_state: Option<Ident> = None;
        let mut default_comb: Vec<CombStmt> = Vec::new();
        let mut default_seq: Vec<Stmt> = Vec::new();
        let mut states: Vec<StateBody> = Vec::new();
        let mut asserts: Vec<AssertDecl> = Vec::new();

        while !self.check_end_fsm() {
            match self.peek_kind() {
                _ if self.check_param() => params.push(self.parse_param_decl()?),
                Some(TokenKind::Port) => ports.push(self.parse_port_decl()?),
                Some(TokenKind::Reg) => regs.push(self.parse_reg_decl()?),
                Some(TokenKind::Wire) => wires.push(self.parse_wire_decl()?),
                Some(TokenKind::Let) => lets.push(self.parse_let_binding()?),
                // `state [A, B, C]` — flat declaration list
                _ if self.check_contextual("state") && self.pos + 1 < self.tokens.len()
                    && self.tokens[self.pos + 1].kind == TokenKind::LBracket => {
                    self.advance(); // consume `state`
                    self.expect(TokenKind::LBracket)?;
                    loop {
                        state_names.push(self.expect_ident()?);
                        if !self.eat(TokenKind::Comma) {
                            break;
                        }
                        // allow trailing comma before `]`
                        if self.check(TokenKind::RBracket) {
                            break;
                        }
                    }
                    self.expect(TokenKind::RBracket)?;
                }
                Some(TokenKind::Default) => {
                    // Peek ahead: `default seq on ...;` sets the seq clock default.
                    // Only treat it as the one-liner form if `seq` is on the same line
                    // (no newline between `default` and `seq`), so that a `default` block
                    // whose first body line is `seq ...` is not misidentified.
                    let default_span = self.tokens[self.pos].span;
                    let next_is_seq_same_line = self.pos + 1 < self.tokens.len()
                        && self.tokens[self.pos + 1].kind == TokenKind::Seq
                        && !self.has_newline_between(default_span.end, self.tokens[self.pos + 1].span.start);
                    if next_is_seq_same_line {
                        self.parse_seq_default_decl()?;
                        continue;
                    }
                    self.advance(); // consume `default`
                    if self.check_contextual("state") {
                        // `default state Name;`
                        self.advance();
                        let ds = self.expect_ident()?;
                        self.expect(TokenKind::Semi)?;
                        default_state = Some(ds);
                    } else {
                        // `default ... end default` block
                        while !(self.check(TokenKind::End)
                            && self.pos + 1 < self.tokens.len()
                            && self.tokens[self.pos + 1].kind == TokenKind::Default)
                        {
                            match self.peek_kind() {
                                Some(TokenKind::Comb) => {
                                    let cb = self.parse_comb_block()?;
                                    default_comb.extend(cb.stmts);
                                }
                                Some(TokenKind::Seq) => {
                                    let rb = self.parse_always_block()?;
                                    default_seq.extend(rb.stmts);
                                }
                                Some(other) => {
                                    return Err(CompileError::unexpected_token(
                                        "comb or seq inside default block",
                                        &other.to_string(),
                                        self.peek_span(),
                                    ));
                                }
                                None => return Err(CompileError::UnexpectedEof),
                            }
                        }
                        self.expect(TokenKind::End)?;
                        self.expect(TokenKind::Default)?;
                    }
                }
                // `state Name ... end state Name` — state body
                _ if self.check_contextual("state") => {
                    states.push(self.parse_state_body()?);
                }
                Some(TokenKind::Assert) | Some(TokenKind::Cover) => {
                    asserts.push(self.parse_assert_decl()?);
                }
                Some(other) => {
                    return Err(CompileError::unexpected_token(
                        "param, port, reg, let, state, default, assert, or cover",
                        &other.to_string(),
                        self.peek_span(),
                    ));
                }
                None => return Err(CompileError::UnexpectedEof),
            }
        }

        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Fsm)?;
        let closing = self.expect_ident()?;
        if closing.name != name.name {
            return Err(CompileError::mismatched_closing(&name.name, &closing.name, closing.span));
        }

        let ds = default_state.ok_or_else(|| {
            CompileError::general("fsm requires `default state Name;`", name.span)
        })?;

        Ok(FsmDecl {
            common: ConstructCommon { name, params, ports, asserts, span: start.merge(closing.span) },
            regs,
            lets,
            wires,
            state_names,
            default_state: ds,
            default_comb,
            default_seq,
            states,
        })
    }

    fn check_end_fsm(&self) -> bool {
        self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos].kind == TokenKind::End
            && self.tokens[self.pos + 1].kind == TokenKind::Fsm
    }

    fn parse_state_body(&mut self) -> Result<StateBody, CompileError> {
        let start = self.expect_contextual("state")?.span;
        let name = self.expect_ident()?;

        let mut comb_stmts = Vec::new();
        let mut seq_stmts = Vec::new();
        let mut transitions = Vec::new();

        while !self.check_end_state() {
            match self.peek_kind() {
                Some(TokenKind::Comb) => {
                    let cb = self.parse_comb_block()?;
                    comb_stmts.extend(cb.stmts);
                }
                Some(TokenKind::Seq) => {
                    let rb = self.parse_always_block()?;
                    seq_stmts.extend(rb.stmts);
                }
                Some(TokenKind::RArrow) => {
                    transitions.push(self.parse_transition()?);
                }
                Some(TokenKind::Let) => {
                    // `let x = expr;` inside state — shorthand for comb assignment
                    let l = self.parse_let_binding()?;
                    comb_stmts.push(CombStmt::Assign(crate::ast::CombAssign {
                        target: Expr { kind: ExprKind::Ident(l.name.name.clone()), span: l.name.span, parenthesized: false },
                        value: l.value,
                        span: l.span,
                    }));
                }
                Some(other) => {
                    return Err(CompileError::unexpected_token(
                        "comb, seq, let, or ->",
                        &other.to_string(),
                        self.peek_span(),
                    ));
                }
                None => return Err(CompileError::UnexpectedEof),
            }
        }

        self.expect(TokenKind::End)?;
        self.expect_contextual("state")?;
        let closing = self.expect_ident()?;
        if closing.name != name.name {
            return Err(CompileError::mismatched_closing(&name.name, &closing.name, closing.span));
        }

        Ok(StateBody {
            span: start.merge(closing.span),
            name,
            comb_stmts,
            seq_stmts,
            transitions,
        })
    }

    fn check_end_state(&self) -> bool {
        self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos].kind == TokenKind::End
            && matches!(&self.tokens[self.pos + 1].kind, TokenKind::Ident(s) if s == "state")
    }


    fn parse_transition(&mut self) -> Result<Transition, CompileError> {
        let start = self.expect(TokenKind::RArrow)?.span;
        let target = self.expect_ident()?;
        // `when <cond>` is optional — omitting it means unconditional (always true)
        let condition = if self.eat(TokenKind::When) {
            self.parse_expr()?
        } else {
            Expr { kind: ExprKind::Bool(true), span: target.span, parenthesized: false }
        };
        self.expect(TokenKind::Semi)?;
        let end_span = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(start);
        Ok(Transition {
            target,
            condition,
            span: start.merge(end_span),
        })
    }

    // ── Pipeline ──────────────────────────────────────────────────────────────

    fn parse_pipeline(&mut self) -> Result<PipelineDecl, CompileError> {
        let start = self.expect(TokenKind::Pipeline)?.span;
        let name = self.expect_ident()?;

        let mut params = Vec::new();
        let mut ports = Vec::new();
        let mut stages = Vec::new();
        let mut stall_conds = Vec::new();
        let mut flush_directives = Vec::new();
        let mut forward_directives = Vec::new();
        let mut asserts: Vec<AssertDecl> = Vec::new();

        while !self.check_end_pipeline() {
            match self.peek_kind() {
                _ if self.check_param() => params.push(self.parse_param_decl()?),
                Some(TokenKind::Port) => ports.push(self.parse_port_decl()?),
                Some(TokenKind::Stage) => stages.push(self.parse_stage_decl()?),
                Some(TokenKind::Stall) => stall_conds.push(self.parse_stall_decl()?),
                Some(TokenKind::Flush) => flush_directives.push(self.parse_flush_decl()?),
                Some(TokenKind::Forward) => forward_directives.push(self.parse_forward_decl()?),
                Some(TokenKind::Assert) | Some(TokenKind::Cover) => {
                    asserts.push(self.parse_assert_decl()?);
                }
                Some(other) => {
                    return Err(CompileError::unexpected_token(
                        "param, port, stage, stall, flush, forward, assert, or cover",
                        &other.to_string(),
                        self.peek_span(),
                    ));
                }
                None => return Err(CompileError::UnexpectedEof),
            }
        }

        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Pipeline)?;
        let closing = self.expect_ident()?;
        if closing.name != name.name {
            return Err(CompileError::mismatched_closing(&name.name, &closing.name, closing.span));
        }

        Ok(PipelineDecl {
            common: ConstructCommon { name, params, ports, asserts, span: start.merge(closing.span) },
            stages,
            stall_conds,
            flush_directives,
            forward_directives,
        })
    }

    fn parse_stage_decl(&mut self) -> Result<StageDecl, CompileError> {
        let start = self.expect(TokenKind::Stage)?.span;
        let name = self.expect_ident()?;
        self.reg_defaults = None; // reset per-stage

        // Optional per-stage stall condition: `stage Fetch stall when <expr>`
        let stall_cond = if self.eat(TokenKind::Stall) {
            self.expect(TokenKind::When)?;
            Some(self.parse_expr()?)
        } else {
            None
        };

        let mut body = Vec::new();

        // Handle todo! stage body
        if self.check(TokenKind::Todo) {
            self.advance(); // consume todo!
            // fall through to end stage
        } else {
            while !self.check_end_stage() {
                match self.peek_kind() {
                    Some(TokenKind::Reg) => {
                        if self.peek_default_at(1) {
                            self.parse_reg_default_decl()?;
                        } else {
                            body.push(ModuleBodyItem::RegDecl(self.parse_reg_decl()?));
                        }
                    }
                    Some(TokenKind::Seq) => {
                        body.push(ModuleBodyItem::RegBlock(self.parse_always_block()?));
                    }
                    Some(TokenKind::Comb) => {
                        body.push(ModuleBodyItem::CombBlock(self.parse_comb_block()?));
                    }
                    Some(TokenKind::Let) => {
                        body.push(ModuleBodyItem::LetBinding(self.parse_let_binding()?));
                    }
                    Some(TokenKind::Inst) => {
                        body.push(ModuleBodyItem::Inst(self.parse_inst()?));
                    }
                    Some(TokenKind::PipeReg) => {
                        body.push(ModuleBodyItem::PipeRegDecl(self.parse_pipe_reg_decl()?));
                    }
                    Some(other) => {
                        return Err(CompileError::unexpected_token(
                            "reg, always, comb, let, inst, pipe_reg, or end stage",
                            &other.to_string(),
                            self.peek_span(),
                        ));
                    }
                    None => return Err(CompileError::UnexpectedEof),
                }
            }
        }

        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Stage)?;
        let closing = self.expect_ident()?;
        if closing.name != name.name {
            return Err(CompileError::mismatched_closing(&name.name, &closing.name, closing.span));
        }

        Ok(StageDecl {
            span: start.merge(closing.span),
            name,
            stall_cond,
            body,
        })
    }

    fn parse_stall_decl(&mut self) -> Result<StallDecl, CompileError> {
        let start = self.expect(TokenKind::Stall)?.span;
        self.expect(TokenKind::When)?;
        let condition = self.parse_expr()?;
        self.expect(TokenKind::Semi)?;
        let end_span = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(start);
        Ok(StallDecl {
            condition,
            span: start.merge(end_span),
        })
    }

    fn parse_flush_decl(&mut self) -> Result<FlushDecl, CompileError> {
        let start = self.expect(TokenKind::Flush)?.span;
        let target_stage = self.expect_ident()?;
        self.expect(TokenKind::When)?;
        let condition = self.parse_expr()?;
        self.expect(TokenKind::Semi)?;
        let end_span = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(start);
        Ok(FlushDecl {
            target_stage,
            condition,
            span: start.merge(end_span),
        })
    }

    fn parse_forward_decl(&mut self) -> Result<ForwardDecl, CompileError> {
        let start = self.expect(TokenKind::Forward)?.span;
        let dest = self.parse_expr()?;
        self.expect(TokenKind::From)?;
        let source = self.parse_expr()?;
        self.expect(TokenKind::When)?;
        let condition = self.parse_expr()?;
        self.expect(TokenKind::Semi)?;
        let end_span = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(start);
        Ok(ForwardDecl {
            dest,
            source,
            condition,
            span: start.merge(end_span),
        })
    }

    fn check_end_pipeline(&self) -> bool {
        self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos].kind == TokenKind::End
            && self.tokens[self.pos + 1].kind == TokenKind::Pipeline
    }

    fn check_end_stage(&self) -> bool {
        self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos].kind == TokenKind::End
            && self.tokens[self.pos + 1].kind == TokenKind::Stage
    }

    // ── FIFO ──────────────────────────────────────────────────────────────────

    fn parse_fifo(&mut self) -> Result<FifoDecl, CompileError> {
        let start = self.expect(TokenKind::Fifo)?.span;
        let name = self.expect_ident()?;

        let mut kind: Option<FifoKind> = None;
        let mut params = Vec::new();
        let mut ports = Vec::new();
        let mut asserts: Vec<AssertDecl> = Vec::new();

        while !self.check_end_fifo() {
            match self.peek_kind() {
                Some(TokenKind::Kind) => {
                    self.advance();
                    let val = self.expect_ident()?;
                    self.expect(TokenKind::Semi)?;
                    kind = Some(match val.name.as_str() {
                        "fifo" => FifoKind::Fifo,
                        "lifo" => FifoKind::Lifo,
                        other => return Err(CompileError::general(
                            &format!("unknown fifo kind `{other}`; expected fifo or lifo"),
                            val.span,
                        )),
                    });
                }
                _ if self.check_param() => params.push(self.parse_param_decl()?),
                Some(TokenKind::Port) => ports.push(self.parse_port_decl()?),
                Some(TokenKind::Assert) | Some(TokenKind::Cover) => {
                    asserts.push(self.parse_assert_decl()?);
                }
                Some(other) => {
                    return Err(CompileError::unexpected_token(
                        "kind, param, port, assert, or cover",
                        &other.to_string(),
                        self.peek_span(),
                    ));
                }
                None => return Err(CompileError::UnexpectedEof),
            }
        }

        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Fifo)?;
        let closing = self.expect_ident()?;
        if closing.name != name.name {
            return Err(CompileError::mismatched_closing(&name.name, &closing.name, closing.span));
        }

        Ok(FifoDecl {
            common: ConstructCommon { name, params, ports, asserts, span: start.merge(closing.span) },
            kind: kind.unwrap_or(FifoKind::Fifo),
        })
    }

    fn check_end_fifo(&self) -> bool {
        self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos].kind == TokenKind::End
            && self.tokens[self.pos + 1].kind == TokenKind::Fifo
    }

    // ── Synchronizer ─────────────────────────────────────────────────────────

    fn parse_synchronizer(&mut self) -> Result<SynchronizerDecl, CompileError> {
        let start = self.expect(TokenKind::Synchronizer)?.span;
        let name = self.expect_ident()?;

        let mut params = Vec::new();
        let mut ports = Vec::new();
        let mut kind = None;

        while !self.check_end_synchronizer() {
            match self.peek_kind() {
                _ if self.check_param() => params.push(self.parse_param_decl()?),
                Some(TokenKind::Port) => ports.push(self.parse_port_decl()?),
                Some(TokenKind::Kind) => {
                    self.advance();
                    let val = self.expect_ident()?;
                    self.expect(TokenKind::Semi)?;
                    kind = Some(match val.name.as_str() {
                        "ff" => SyncKind::Ff,
                        "gray" => SyncKind::Gray,
                        "handshake" => SyncKind::Handshake,
                        "reset" => SyncKind::Reset,
                        "pulse" => SyncKind::Pulse,
                        other => return Err(CompileError::general(
                            &format!("unknown synchronizer kind `{other}`; expected ff, gray, handshake, reset, or pulse"),
                            val.span,
                        )),
                    });
                }
                Some(other) => {
                    return Err(CompileError::unexpected_token(
                        "kind, param, or port",
                        &other.to_string(),
                        self.peek_span(),
                    ));
                }
                None => return Err(CompileError::UnexpectedEof),
            }
        }

        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Synchronizer)?;
        let closing = self.expect_ident()?;
        if closing.name != name.name {
            return Err(CompileError::mismatched_closing(&name.name, &closing.name, closing.span));
        }

        Ok(SynchronizerDecl {
            span: start.merge(closing.span),
            name,
            kind: kind.unwrap_or(SyncKind::Ff),
            params,
            ports,
        })
    }

    fn check_end_synchronizer(&self) -> bool {
        self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos].kind == TokenKind::End
            && self.tokens[self.pos + 1].kind == TokenKind::Synchronizer
    }

    // ── Clock Gate ────────────────────────────────────────────────────────────

    fn parse_clkgate(&mut self) -> Result<ClkGateDecl, CompileError> {
        let start = self.expect(TokenKind::Clkgate)?.span;
        let name = self.expect_ident()?;

        let mut params = Vec::new();
        let mut ports = Vec::new();
        let mut kind = None;

        while !self.check_end_clkgate() {
            match self.peek_kind() {
                _ if self.check_param() => params.push(self.parse_param_decl()?),
                Some(TokenKind::Port) => ports.push(self.parse_port_decl()?),
                Some(TokenKind::Kind) => {
                    self.advance();
                    // 'and' and 'latch' are both keyword tokens, handle both specially
                    let span = self.peek_span();
                    let kind_val = match self.peek_kind() {
                        Some(TokenKind::And) => { self.advance(); "and" }
                        Some(TokenKind::Latch) => { self.advance(); "latch" }
                        Some(TokenKind::Ident(_)) => {
                            let val = self.expect_ident()?;
                            match val.name.as_str() {
                                "latch" => "latch",
                                other => return Err(CompileError::general(
                                    &format!("unknown clkgate kind `{other}`; expected latch or and"),
                                    val.span,
                                )),
                            }
                        }
                        _ => return Err(CompileError::unexpected_token("latch or and", &format!("{:?}", self.peek_kind()), span)),
                    };
                    self.expect(TokenKind::Semi)?;
                    kind = Some(match kind_val {
                        "latch" => ClkGateKind::Latch,
                        "and" => ClkGateKind::And,
                        _ => unreachable!(),
                    });
                }
                Some(other) => {
                    return Err(CompileError::unexpected_token(
                        "kind, param, or port",
                        &other.to_string(),
                        self.peek_span(),
                    ));
                }
                None => return Err(CompileError::UnexpectedEof),
            }
        }

        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Clkgate)?;
        let closing = self.expect_ident()?;
        if closing.name != name.name {
            return Err(CompileError::mismatched_closing(&name.name, &closing.name, closing.span));
        }

        Ok(ClkGateDecl {
            span: start.merge(closing.span),
            name,
            kind: kind.unwrap_or(ClkGateKind::Latch),
            params,
            ports,
        })
    }

    fn check_end_clkgate(&self) -> bool {
        self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos].kind == TokenKind::End
            && self.tokens[self.pos + 1].kind == TokenKind::Clkgate
    }

    // ── RAM ───────────────────────────────────────────────────────────────────

    fn parse_ram(&mut self) -> Result<RamDecl, CompileError> {
        let start = self.expect(TokenKind::Ram)?.span;
        let name = self.expect_ident()?;

        let mut params = Vec::new();
        let mut ports = Vec::new();
        let mut kind: Option<RamKind> = None;
        let mut latency: Option<u32> = None;
        let mut write_mode: Option<RamWriteMode> = None;
        let mut collision: Option<RamCollision> = None;
        let mut store_vars = Vec::new();
        let mut port_groups = Vec::new();
        let mut init: Option<RamInit> = None;
        let mut asserts: Vec<AssertDecl> = Vec::new();

        // Phase 1: attributes (kind, read, write, collision, init)
        while !self.check_end_ram() {
            if self.check(TokenKind::Param) || self.check(TokenKind::Port)
                || self.check(TokenKind::Store)
                || self.check(TokenKind::Assert) || self.check(TokenKind::Cover) {
                break;
            }
            if self.check(TokenKind::Init) {
                init = Some(self.parse_ram_init()?);
            } else if self.check(TokenKind::Kind) {
                self.advance();
                let val = self.expect_ident()?;
                self.expect(TokenKind::Semi)?;
                kind = Some(match val.name.as_str() {
                    "single" => RamKind::Single,
                    "simple_dual" => RamKind::SimpleDual,
                    "true_dual" => RamKind::TrueDual,
                    "rom" => RamKind::Rom,
                    other => return Err(CompileError::general(
                        &format!("unknown ram kind `{other}`; expected single, simple_dual, true_dual, or rom"),
                        val.span,
                    )),
                });
            } else if self.check(TokenKind::Latency) {
                self.advance();
                let lit_span = self.peek_span();
                let val = match self.peek_kind() {
                    Some(TokenKind::DecLiteral(s)) => {
                        let v = s.parse::<u32>().map_err(|_| CompileError::general(
                            "expected integer after `latency`", lit_span))?;
                        self.advance(); v
                    }
                    _ => return Err(CompileError::general(
                        "expected integer after `latency`",
                        lit_span,
                    )),
                };
                self.expect(TokenKind::Semi)?;
                latency = Some(val);
            } else if self.check_ident("write") {
                self.advance();
                self.expect(TokenKind::Colon)?;
                let val = self.expect_ident()?;
                self.expect(TokenKind::Semi)?;
                write_mode = Some(match val.name.as_str() {
                    "first" => RamWriteMode::WriteFirst,
                    "read_first" => RamWriteMode::ReadFirst,
                    "no_change" => RamWriteMode::NoChange,
                    other => return Err(CompileError::general(
                        &format!("unknown write mode `{other}`; expected first, read_first, or no_change"),
                        val.span,
                    )),
                });
            } else if self.check_ident("collision") {
                self.advance();
                self.expect(TokenKind::Colon)?;
                let val = self.expect_ident()?;
                self.expect(TokenKind::Semi)?;
                collision = Some(match val.name.as_str() {
                    "port_a_wins" => RamCollision::PortAWins,
                    "port_b_wins" => RamCollision::PortBWins,
                    "undefined" => RamCollision::Undefined,
                    other => return Err(CompileError::general(
                        &format!("unknown collision policy `{other}`"),
                        val.span,
                    )),
                });
            } else {
                return Err(CompileError::unexpected_token(
                    "kind, latency, write, collision, or init",
                    &self.peek_kind().map(|k| k.to_string()).unwrap_or("EOF".into()),
                    self.peek_span(),
                ));
            }
        }

        // Phase 2: params
        while !self.check_end_ram() && self.check(TokenKind::Param) {
            params.push(self.parse_param_decl()?);
        }

        // Phase 3: ports, store, port groups, assert/cover
        while !self.check_end_ram() {
            match self.peek_kind() {
                Some(TokenKind::Port) => {
                    ports.push(self.parse_port_decl()?);
                }
                Some(TokenKind::Ports) => {
                    port_groups.push(self.parse_ram_port_group()?);
                }
                Some(TokenKind::Store) => {
                    store_vars = self.parse_store_block()?;
                }
                Some(TokenKind::Init) => {
                    init = Some(self.parse_ram_init()?);
                }
                Some(TokenKind::Assert) | Some(TokenKind::Cover) => {
                    asserts.push(self.parse_assert_decl()?);
                }
                Some(other) => return Err(CompileError::unexpected_token(
                    "port, store, init, assert, or cover",
                    &other.to_string(),
                    self.peek_span(),
                )),
                None => return Err(CompileError::UnexpectedEof),
            }
        }

        let k = kind.ok_or_else(|| CompileError::general(
            "ram is missing required `kind` directive",
            name.span,
        ))?;
        let lat = latency.ok_or_else(|| CompileError::general(
            "ram is missing required `latency` directive",
            name.span,
        ))?;

        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Ram)?;
        let closing = self.expect_ident()?;
        if closing.name != name.name {
            return Err(CompileError::mismatched_closing(&name.name, &closing.name, closing.span));
        }

        Ok(RamDecl {
            common: ConstructCommon { name, params, ports, asserts, span: start.merge(closing.span) },
            kind: k,
            latency: lat,
            write_mode,
            collision,
            store_vars,
            port_groups,
            init,
        })
    }

    fn parse_store_block(&mut self) -> Result<Vec<RamStoreVar>, CompileError> {
        self.expect(TokenKind::Store)?;
        let mut vars = Vec::new();
        while !self.check_end_store() {
            let name = self.expect_ident()?;
            let start = name.span;
            self.expect(TokenKind::Colon)?;
            let ty = self.parse_type_expr()?;
            self.expect(TokenKind::Semi)?;
            let end_span = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(start);
            vars.push(RamStoreVar { name, ty, span: start.merge(end_span) });
        }
        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Store)?;
        Ok(vars)
    }

    fn check_end_store(&self) -> bool {
        self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos].kind == TokenKind::End
            && self.tokens[self.pos + 1].kind == TokenKind::Store
    }

    fn parse_ram_port_group(&mut self) -> Result<RamPortGroup, CompileError> {
        let start = self.expect(TokenKind::Ports)?.span;
        let name = self.expect_ident()?;
        let mut signals = Vec::new();
        while !self.check_end_port_group() {
            signals.push(self.parse_inner_signal()?);
        }
        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Ports)?;
        let closing = self.expect_ident()?;
        if closing.name != name.name {
            return Err(CompileError::mismatched_closing(&name.name, &closing.name, closing.span));
        }
        Ok(RamPortGroup {
            span: start.merge(closing.span),
            name,
            signals,
        })
    }

    fn check_end_port_group(&self) -> bool {
        self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos].kind == TokenKind::End
            && self.tokens[self.pos + 1].kind == TokenKind::Ports
    }

    /// Parse a signal declaration inside a port group: `name: in|out TypeExpr;`
    fn parse_inner_signal(&mut self) -> Result<PortDecl, CompileError> {
        let name = self.expect_ident()?;
        let start = name.span;
        self.expect(TokenKind::Colon)?;
        let direction = if self.eat_contextual("in") {
            Direction::In
        } else if self.eat_contextual("out") {
            Direction::Out
        } else {
            return Err(CompileError::unexpected_token(
                "in or out",
                &self.peek_kind().map(|k| k.to_string()).unwrap_or("EOF".into()),
                self.peek_span(),
            ));
        };
        let ty = self.parse_type_expr()?;
        self.expect(TokenKind::Semi)?;
        let end_span = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(start);
        Ok(PortDecl { name, direction, ty, default: None, reg_info: None, bus_info: None, shared: None, span: start.merge(end_span) })
    }

    fn parse_ram_init(&mut self) -> Result<RamInit, CompileError> {
        self.expect(TokenKind::Init)?;
        self.expect(TokenKind::Colon)?;
        match self.peek_kind() {
            Some(TokenKind::Ident(ref s)) => {
                let s = s.clone();
                match s.as_str() {
                    "zero" => { self.advance(); self.expect(TokenKind::Semi)?; Ok(RamInit::Zero) }
                    "none" => { self.advance(); self.expect(TokenKind::Semi)?; Ok(RamInit::None) }
                    "file" => {
                        self.advance();
                        self.expect(TokenKind::LParen)?;
                        let path = match self.peek_kind() {
                            Some(TokenKind::StringLit(s)) => { let s = s.clone(); self.advance(); s }
                            _ => {
                                // Fallback: scan tokens until , or ) for unquoted paths
                                let mut p = String::new();
                                while !self.check(TokenKind::Comma) && !self.check(TokenKind::RParen) && !self.at_end() {
                                    let tok = self.advance();
                                    p.push_str(&tok.kind.to_string());
                                }
                                p
                            }
                        };
                        let format = if self.check(TokenKind::Comma) {
                            self.advance();
                            let fmt_id = self.expect_ident()?;
                            match fmt_id.name.as_str() {
                                "hex" => FileFormat::Hex,
                                "bin" => FileFormat::Bin,
                                other => return Err(CompileError::general(
                                    &format!("unknown file format `{other}`; expected hex or bin"),
                                    fmt_id.span,
                                )),
                            }
                        } else {
                            FileFormat::Hex // default
                        };
                        self.expect(TokenKind::RParen)?;
                        self.expect(TokenKind::Semi)?;
                        Ok(RamInit::File(path, format))
                    }
                    "value" => {
                        self.advance();
                        let expr = self.parse_expr()?;
                        self.expect(TokenKind::Semi)?;
                        Ok(RamInit::Value(expr))
                    }
                    other => Err(CompileError::general(
                        &format!("unknown init mode `{other}`; expected zero, none, file, value, or [...]"),
                        self.peek_span(),
                    )),
                }
            }
            Some(TokenKind::LBracket) => {
                self.advance();
                let mut values = Vec::new();
                while !self.check(TokenKind::RBracket) && !self.at_end() {
                    let expr = self.parse_expr()?;
                    let val = match &expr.kind {
                        ExprKind::Literal(LitKind::Dec(v)) => *v,
                        ExprKind::Literal(LitKind::Hex(v)) => *v,
                        ExprKind::Literal(LitKind::Bin(v)) => *v,
                        ExprKind::Literal(LitKind::Sized(_, v)) => *v,
                        _ => return Err(CompileError::general(
                            "init array elements must be integer literals",
                            expr.span,
                        )),
                    };
                    values.push(val);
                    if self.check(TokenKind::Comma) { self.advance(); }
                }
                self.expect(TokenKind::RBracket)?;
                self.expect(TokenKind::Semi)?;
                Ok(RamInit::Array(values))
            }
            Some(other) => Err(CompileError::unexpected_token(
                "zero, none, file, value, or [...]",
                &other.to_string(),
                self.peek_span(),
            )),
            None => Err(CompileError::UnexpectedEof),
        }
    }

    fn check_end_ram(&self) -> bool {
        self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos].kind == TokenKind::End
            && self.tokens[self.pos + 1].kind == TokenKind::Ram
    }

    // ── Counter ───────────────────────────────────────────────────────────────

    fn parse_counter(&mut self) -> Result<CounterDecl, CompileError> {
        let start = self.expect(TokenKind::Counter)?.span;
        let name = self.expect_ident()?;

        let mut params = Vec::new();
        let mut ports = Vec::new();
        let mut mode: Option<CounterMode> = None;
        let mut direction: Option<CounterDirection> = None;
        let mut init: Option<Expr> = None;

        // Phase 1: attributes (kind, direction, init) — must come first
        while !self.check_end_of(TokenKind::Counter) {
            if self.check(TokenKind::Param) || self.check(TokenKind::Port)
                || self.check(TokenKind::Assert) || self.check(TokenKind::Cover) {
                break;
            }
            if self.check(TokenKind::Init) {
                self.advance();
                self.expect(TokenKind::Colon)?;
                init = Some(self.parse_expr()?);
                self.expect(TokenKind::Semi)?;
            } else if self.check(TokenKind::Kind) {
                self.advance();
                let val = self.expect_ident()?;
                self.expect(TokenKind::Semi)?;
                mode = Some(match val.name.as_str() {
                    "wrap"     => CounterMode::Wrap,
                    "saturate" => CounterMode::Saturate,
                    "gray"     => CounterMode::Gray,
                    "one_hot"  => CounterMode::OneHot,
                    "johnson"  => CounterMode::Johnson,
                    other => return Err(CompileError::general(
                        &format!("unknown counter kind `{other}`; expected wrap, saturate, gray, one_hot, or johnson"),
                        val.span,
                    )),
                });
            } else if self.check_ident("direction") {
                self.advance();
                self.expect(TokenKind::Colon)?;
                let val = self.expect_ident()?;
                self.expect(TokenKind::Semi)?;
                direction = Some(match val.name.as_str() {
                    "up"      => CounterDirection::Up,
                    "down"    => CounterDirection::Down,
                    "up_down" => CounterDirection::UpDown,
                    other => return Err(CompileError::general(
                        &format!("unknown counter direction `{other}`"),
                        val.span,
                    )),
                });
            } else {
                return Err(CompileError::unexpected_token(
                    "kind, direction, or init",
                    &self.peek_kind().map(|k| k.to_string()).unwrap_or("EOF".into()),
                    self.peek_span(),
                ));
            }
        }

        // Phase 2: params
        while !self.check_end_of(TokenKind::Counter) && self.check(TokenKind::Param) {
            params.push(self.parse_param_decl()?);
        }

        // Phase 3: ports (and assert/cover)
        let mut asserts: Vec<AssertDecl> = Vec::new();
        while !self.check_end_of(TokenKind::Counter) {
            match self.peek_kind() {
                Some(TokenKind::Port) => ports.push(self.parse_port_decl()?),
                Some(TokenKind::Assert) | Some(TokenKind::Cover) => {
                    asserts.push(self.parse_assert_decl()?);
                }
                Some(other) => return Err(CompileError::unexpected_token(
                    "port, assert, or cover",
                    &other.to_string(),
                    self.peek_span(),
                )),
                None => return Err(CompileError::UnexpectedEof),
            }
        }

        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Counter)?;
        let closing = self.expect_ident()?;
        if closing.name != name.name {
            return Err(CompileError::mismatched_closing(&name.name, &closing.name, closing.span));
        }

        let mode = mode.unwrap_or(CounterMode::Wrap);
        let direction = direction.unwrap_or(CounterDirection::Up);
        Ok(CounterDecl {
            common: ConstructCommon { name, params, ports, asserts, span: start.merge(closing.span) },
            mode, direction, init,
        })
    }

    fn check_end_of(&self, kw: TokenKind) -> bool {
        self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos].kind == TokenKind::End
            && self.tokens[self.pos + 1].kind == kw
    }

    // ── Arbiter ───────────────────────────────────────────────────────────────

    fn parse_arbiter(&mut self) -> Result<ArbiterDecl, CompileError> {
        let start = self.expect(TokenKind::Arbiter)?.span;
        let name = self.expect_ident()?;

        let mut params = Vec::new();
        let mut ports = Vec::new();
        let mut port_arrays = Vec::new();
        let mut policy: Option<ArbiterPolicy> = None;
        let mut hook: Option<crate::ast::ArbiterHookDecl> = None;
        let mut latency: u32 = 1;

        // Phase 1: attributes (policy, latency)
        while !self.check_end_of(TokenKind::Arbiter) {
            if self.check(TokenKind::Param) || self.check(TokenKind::Port)
                || self.check(TokenKind::Ports) || self.check(TokenKind::Hook)
                || self.check(TokenKind::Assert) || self.check(TokenKind::Cover) {
                break;
            }
            if self.check(TokenKind::Latency) {
                self.advance();
                let lit_span = self.peek_span();
                let val = match self.peek_kind() {
                    Some(TokenKind::DecLiteral(s)) => {
                        let v = s.parse::<u32>().map_err(|_| CompileError::general(
                            "expected integer after `latency`", lit_span))?;
                        self.advance(); v
                    }
                    _ => return Err(CompileError::general(
                        "expected integer after `latency`",
                        lit_span,
                    )),
                };
                self.expect(TokenKind::Semi)?;
                latency = val;
            } else if self.check_ident("policy") {
                self.advance();
                let val = self.expect_ident()?;
                self.expect(TokenKind::Semi)?;
                policy = Some(match val.name.as_str() {
                    "round_robin" => ArbiterPolicy::RoundRobin,
                    "priority"    => ArbiterPolicy::Priority,
                    "lru"         => ArbiterPolicy::Lru,
                    "weighted" => {
                        let w = Expr {
                            kind: ExprKind::Literal(LitKind::Dec(1)),
                            span: val.span, parenthesized: false };
                        ArbiterPolicy::Weighted(w)
                    }
                    _ => ArbiterPolicy::Custom(val),
                });
            } else {
                return Err(CompileError::unexpected_token(
                    "policy",
                    &self.peek_kind().map(|k| k.to_string()).unwrap_or("EOF".into()),
                    self.peek_span(),
                ));
            }
        }

        // Phase 2: params
        while !self.check_end_of(TokenKind::Arbiter) && self.check(TokenKind::Param) {
            params.push(self.parse_param_decl()?);
        }

        // Phase 3: ports, port arrays, hook, assert/cover
        let mut asserts: Vec<AssertDecl> = Vec::new();
        while !self.check_end_of(TokenKind::Arbiter) {
            match self.peek_kind() {
                Some(TokenKind::Port) => ports.push(self.parse_port_decl()?),
                Some(TokenKind::Ports) => port_arrays.push(self.parse_port_array()?),
                Some(TokenKind::Hook) => {
                    hook = Some(self.parse_arbiter_hook()?);
                }
                Some(TokenKind::Assert) | Some(TokenKind::Cover) => {
                    asserts.push(self.parse_assert_decl()?);
                }
                Some(other) => return Err(CompileError::unexpected_token(
                    "port, ports, hook, assert, or cover",
                    &other.to_string(),
                    self.peek_span(),
                )),
                None => return Err(CompileError::UnexpectedEof),
            }
        }

        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Arbiter)?;
        let closing = self.expect_ident()?;
        if closing.name != name.name {
            return Err(CompileError::mismatched_closing(&name.name, &closing.name, closing.span));
        }

        Ok(ArbiterDecl {
            common: ConstructCommon { name, params, ports, asserts, span: start.merge(closing.span) },
            port_arrays,
            policy: policy.unwrap_or(ArbiterPolicy::RoundRobin),
            hook,
            latency,
        })
    }

    /// Parse `hook grant_select(req_mask: UInt<N>, ...) -> UInt<N> = FnName(arg1, ...);`
    fn parse_arbiter_hook(&mut self) -> Result<crate::ast::ArbiterHookDecl, CompileError> {
        let start = self.expect(TokenKind::Hook)?.span;
        let hook_name = self.expect_ident()?;

        // Parse formal parameters: (name: Type, ...)
        self.expect(TokenKind::LParen)?;
        let mut params = Vec::new();
        while !self.check(TokenKind::RParen) {
            if !params.is_empty() {
                self.expect(TokenKind::Comma)?;
            }
            let pname = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            let pty = self.parse_type_expr()?;
            params.push(crate::ast::FunctionArg { name: pname, ty: pty });
        }
        self.expect(TokenKind::RParen)?;

        // Parse -> RetType
        self.expect(TokenKind::RArrow)?;
        let ret_ty = self.parse_type_expr()?;

        // Parse = FnName(arg1, arg2, ...);
        self.expect(TokenKind::Eq)?;
        let fn_name = self.expect_ident()?;
        self.expect(TokenKind::LParen)?;
        let mut fn_args = Vec::new();
        while !self.check(TokenKind::RParen) {
            if !fn_args.is_empty() {
                self.expect(TokenKind::Comma)?;
            }
            fn_args.push(self.expect_ident()?);
        }
        self.expect(TokenKind::RParen)?;
        let end = self.expect(TokenKind::Semi)?.span;

        Ok(crate::ast::ArbiterHookDecl {
            hook_name,
            params,
            ret_ty,
            fn_name,
            fn_args,
            span: start.merge(end),
        })
    }

    /// Parse `hook name(args) -> RetType = FnName(args);` inside a module
    fn parse_module_hook_decl(&mut self) -> Result<crate::ast::ModuleHookDecl, CompileError> {
        let start = self.expect(TokenKind::Hook)?.span;
        let hook_name = self.expect_ident()?;

        self.expect(TokenKind::LParen)?;
        let mut params = Vec::new();
        while !self.check(TokenKind::RParen) {
            if !params.is_empty() { self.expect(TokenKind::Comma)?; }
            let pname = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            let pty = self.parse_type_expr()?;
            params.push(crate::ast::FunctionArg { name: pname, ty: pty });
        }
        self.expect(TokenKind::RParen)?;

        self.expect(TokenKind::RArrow)?;
        let ret_ty = self.parse_type_expr()?;

        self.expect(TokenKind::Eq)?;
        let fn_name = self.expect_ident()?;
        self.expect(TokenKind::LParen)?;
        let mut fn_args = Vec::new();
        while !self.check(TokenKind::RParen) {
            if !fn_args.is_empty() { self.expect(TokenKind::Comma)?; }
            fn_args.push(self.expect_ident()?);
        }
        self.expect(TokenKind::RParen)?;
        let end = self.expect(TokenKind::Semi)?.span;

        Ok(crate::ast::ModuleHookDecl {
            hook_name,
            params,
            ret_ty,
            fn_name,
            fn_args,
            span: start.merge(end),
        })
    }

    // ── Template ─────────────────────────────────────────────────────────────

    fn parse_template(&mut self) -> Result<crate::ast::TemplateDecl, CompileError> {
        let start = self.expect(TokenKind::Template)?.span;
        let name = self.expect_ident()?;

        let mut params = Vec::new();
        let mut ports = Vec::new();
        let mut port_arrays = Vec::new();
        let mut hooks = Vec::new();

        while !self.check_end_of(TokenKind::Template) {
            match self.peek_kind() {
                _ if self.check_param() => params.push(self.parse_param_decl()?),
                Some(TokenKind::Port) => ports.push(self.parse_port_decl()?),
                Some(TokenKind::Ports) => port_arrays.push(self.parse_port_array()?),
                Some(TokenKind::Hook) => hooks.push(self.parse_template_hook_decl()?),
                Some(other) => return Err(CompileError::unexpected_token(
                    "param, port, ports, or hook",
                    &other.to_string(),
                    self.peek_span(),
                )),
                None => return Err(CompileError::UnexpectedEof),
            }
        }

        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Template)?;
        let closing = self.expect_ident()?;
        if closing.name != name.name {
            return Err(CompileError::mismatched_closing(&name.name, &closing.name, closing.span));
        }

        let span = start.merge(closing.span);
        Ok(crate::ast::TemplateDecl { name, params, ports, port_arrays, hooks, span })
    }

    /// Parse `hook name(args) -> RetType;` (no binding — template signature only)
    fn parse_template_hook_decl(&mut self) -> Result<crate::ast::TemplateHookDecl, CompileError> {
        let start = self.expect(TokenKind::Hook)?.span;
        let name = self.expect_ident()?;

        self.expect(TokenKind::LParen)?;
        let mut params = Vec::new();
        while !self.check(TokenKind::RParen) {
            if !params.is_empty() { self.expect(TokenKind::Comma)?; }
            let pname = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            let pty = self.parse_type_expr()?;
            params.push(crate::ast::FunctionArg { name: pname, ty: pty });
        }
        self.expect(TokenKind::RParen)?;

        self.expect(TokenKind::RArrow)?;
        let ret_ty = self.parse_type_expr()?;
        let end = self.expect(TokenKind::Semi)?.span;

        Ok(crate::ast::TemplateHookDecl {
            name,
            params,
            ret_ty,
            span: start.merge(end),
        })
    }

    /// Parse `ports[N] name ... end ports name`
    fn parse_port_array(&mut self) -> Result<PortArrayDecl, CompileError> {
        let start = self.expect(TokenKind::Ports)?.span;
        self.expect(TokenKind::LBracket)?;
        let count_expr = self.parse_expr()?;
        self.expect(TokenKind::RBracket)?;
        let name = self.expect_ident()?;
        let mut signals = Vec::new();
        while !self.check_end_ports() {
            signals.push(self.parse_inner_signal()?);
        }
        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Ports)?;
        let closing = self.expect_ident()?;
        if closing.name != name.name {
            return Err(CompileError::mismatched_closing(&name.name, &closing.name, closing.span));
        }
        Ok(PortArrayDecl {
            span: start.merge(closing.span),
            count_expr,
            name,
            signals,
        })
    }

    fn check_end_ports(&self) -> bool {
        self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos].kind == TokenKind::End
            && self.tokens[self.pos + 1].kind == TokenKind::Ports
    }

    // ── Regfile ───────────────────────────────────────────────────────────────

    fn parse_regfile(&mut self) -> Result<RegfileDecl, CompileError> {
        let start = self.expect(TokenKind::Regfile)?.span;
        let name = self.expect_ident()?;

        let mut params = Vec::new();
        let mut ports = Vec::new();
        let mut read_ports: Option<PortArrayDecl> = None;
        let mut write_ports: Option<PortArrayDecl> = None;
        let mut inits: Vec<RegfileInit> = Vec::new();
        let mut forward_write_before_read = false;
        let mut asserts: Vec<AssertDecl> = Vec::new();

        while !self.check_end_of(TokenKind::Regfile) {
            match self.peek_kind() {
                _ if self.check_param() => params.push(self.parse_param_decl()?),
                Some(TokenKind::Port) => ports.push(self.parse_port_decl()?),
                Some(TokenKind::Ports) => {
                    let arr = self.parse_port_array()?;
                    match arr.name.name.as_str() {
                        "read"  => read_ports  = Some(arr),
                        "write" => write_ports = Some(arr),
                        other => {
                            // accept any name; use name to detect
                            if other.contains("read") {
                                read_ports = Some(arr);
                            } else {
                                write_ports = Some(arr);
                            }
                        }
                    }
                }
                Some(TokenKind::Init) => {
                    let init_span = self.peek_span();
                    self.advance(); // consume "init"
                    self.expect(TokenKind::LBracket)?;
                    let index = self.parse_expr()?;
                    self.expect(TokenKind::RBracket)?;
                    self.expect(TokenKind::Eq)?;
                    let value = self.parse_expr()?;
                    self.expect(TokenKind::Semi)?;
                    inits.push(RegfileInit { index, value, span: init_span });
                }
                Some(TokenKind::Forward) => {
                    self.advance(); // consume "forward"
                    // `write_before_read: true;` or similar
                    while !self.check(TokenKind::Semi) && !self.at_end() {
                        if let Some(TokenKind::True) = self.peek_kind() {
                            forward_write_before_read = true;
                        }
                        self.advance();
                    }
                    self.eat(TokenKind::Semi);
                }
                Some(TokenKind::Assert) | Some(TokenKind::Cover) => {
                    asserts.push(self.parse_assert_decl()?);
                }
                Some(other) => return Err(CompileError::unexpected_token(
                    "param, port, ports, init, forward, assert, or cover",
                    &other.to_string(),
                    self.peek_span(),
                )),
                None => return Err(CompileError::UnexpectedEof),
            }
        }

        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Regfile)?;
        let closing = self.expect_ident()?;
        if closing.name != name.name {
            return Err(CompileError::mismatched_closing(&name.name, &closing.name, closing.span));
        }

        Ok(RegfileDecl {
            common: ConstructCommon { name, params, ports, asserts, span: start.merge(closing.span) },
            read_ports,
            write_ports,
            inits,
            forward_write_before_read,
        })
    }

    // --- Token utilities ---
    fn peek_kind(&self) -> Option<TokenKind> {
        self.tokens.get(self.pos).map(|t| t.kind.clone())
    }

    fn peek_kind_at(&self, idx: usize) -> Option<TokenKind> {
        self.tokens.get(idx).map(|t| t.kind.clone())
    }

    fn peek_span(&self) -> Span {
        self.tokens
            .get(self.pos)
            .map(|t| t.span)
            .unwrap_or(Span::new(0, 0))
    }

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn check(&self, kind: TokenKind) -> bool {
        self.peek_kind().as_ref() == Some(&kind)
    }

    fn check_end_keyword(&self) -> bool {
        self.check(TokenKind::End)
    }

    /// Check if the next token is an identifier with a specific name.
    fn check_ident(&self, name: &str) -> bool {
        matches!(self.peek_kind(), Some(TokenKind::Ident(ref s)) if s == name)
    }

    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.check(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// Eat a contextual keyword (lexed as Ident, matched by name).
    fn eat_contextual(&mut self, name: &str) -> bool {
        if self.check_ident(name) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// Expect a contextual keyword (lexed as Ident, matched by name).
    fn expect_contextual(&mut self, name: &str) -> Result<Token, CompileError> {
        if self.check_ident(name) {
            Ok(self.advance())
        } else {
            let span = self.tokens.get(self.pos).map(|t| t.span).unwrap_or(Span { start: 0, end: 0 });
            Err(CompileError::general(
                &format!("expected `{}`", name),
                span,
            ))
        }
    }

    /// Check if the next token(s) start a param decl: `param` or `local param`.
    fn check_param(&self) -> bool {
        if self.check(TokenKind::Param) { return true; }
        if self.check_ident("local") {
            if let Some(t) = self.tokens.get(self.pos + 1) {
                return t.kind == TokenKind::Param;
            }
        }
        false
    }

    /// Check if next token is a contextual keyword (for lookahead).
    fn check_contextual(&self, name: &str) -> bool {
        self.check_ident(name)
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens[self.pos].clone();
        self.pos += 1;
        tok
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token, CompileError> {
        if self.check(kind.clone()) {
            Ok(self.advance())
        } else {
            Err(CompileError::unexpected_token(
                &kind.to_string(),
                &self.peek_kind().map(|k| k.to_string()).unwrap_or("EOF".into()),
                self.peek_span(),
            ))
        }
    }

    fn expect_ident(&mut self) -> Result<Ident, CompileError> {
        // Contextual keywords that are valid identifiers in non-keyword positions.
        let contextual_name = match self.peek_kind() {
            Some(TokenKind::Op)        => Some("op"),
            Some(TokenKind::Track)     => Some("track"),
            Some(TokenKind::Latency)   => Some("latency"),
            Some(TokenKind::Pipelined) => Some("pipelined"),
            Some(TokenKind::Kind)      => Some("kind"),
            _ => None,
        };
        if let Some(name) = contextual_name {
            let tok = self.advance();
            return Ok(Ident::new(name.to_string(), tok.span));
        }
        match self.peek_kind() {
            Some(TokenKind::Ident(name)) => {
                let tok = self.advance();
                Ok(Ident::new(name, tok.span))
            }
            other => Err(CompileError::unexpected_token(
                "identifier",
                &other.map(|k| k.to_string()).unwrap_or("EOF".into()),
                self.peek_span(),
            )),
        }
    }

    // ── Linklist ──────────────────────────────────────────────────────────────

    fn parse_linklist(&mut self) -> Result<LinklistDecl, CompileError> {
        let start = self.expect(TokenKind::Linklist)?.span;
        let name = self.expect_ident()?;

        let mut params = Vec::new();
        let mut ports = Vec::new();
        let mut kind: Option<LinklistKind> = None;
        let mut track_tail = false;
        let mut track_length = false;
        let mut ops = Vec::new();
        let mut asserts: Vec<AssertDecl> = Vec::new();

        while !self.check_end_linklist() {
            match self.peek_kind() {
                _ if self.check_param() => params.push(self.parse_param_decl()?),
                Some(TokenKind::Port) => ports.push(self.parse_port_decl()?),
                Some(TokenKind::Kind) => {
                    self.advance(); // consume 'kind'
                    let kw = self.expect_ident()?;
                    kind = Some(match kw.name.as_str() {
                        "singly"           => LinklistKind::Singly,
                        "doubly"           => LinklistKind::Doubly,
                        "circular_singly"  => LinklistKind::CircularSingly,
                        "circular_doubly"  => LinklistKind::CircularDoubly,
                        other => return Err(CompileError::unexpected_token(
                            "singly, doubly, circular_singly, or circular_doubly",
                            other, kw.span,
                        )),
                    });
                    self.eat(TokenKind::Semi);
                }
                Some(TokenKind::Track) => {
                    self.advance(); // consume 'track'
                    let field = self.expect_ident()?;
                    self.expect(TokenKind::Colon)?;
                    let val = match self.peek_kind() {
                        Some(TokenKind::True)  => { self.advance(); true }
                        Some(TokenKind::False) => { self.advance(); false }
                        Some(TokenKind::Ident(ref s)) if s == "true"  => { self.advance(); true }
                        Some(TokenKind::Ident(ref s)) if s == "false" => { self.advance(); false }
                        other => return Err(CompileError::unexpected_token(
                            "true or false",
                            &other.map(|k| k.to_string()).unwrap_or("EOF".into()),
                            self.peek_span(),
                        )),
                    };
                    self.eat(TokenKind::Semi);
                    match field.name.as_str() {
                        "tail"   => track_tail   = val,
                        "length" => track_length = val,
                        other => return Err(CompileError::unexpected_token(
                            "tail or length", other, field.span,
                        )),
                    }
                }
                Some(TokenKind::Op) => ops.push(self.parse_op_decl()?),
                Some(TokenKind::Assert) | Some(TokenKind::Cover) => {
                    asserts.push(self.parse_assert_decl()?);
                }
                Some(other) => return Err(CompileError::unexpected_token(
                    "param, port, kind, track, op, assert, or cover", &other.to_string(), self.peek_span(),
                )),
                None => return Err(CompileError::UnexpectedEof),
            }
        }

        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Linklist)?;
        let closing = self.expect_ident()?;
        if closing.name != name.name {
            return Err(CompileError::mismatched_closing(&name.name, &closing.name, closing.span));
        }

        Ok(LinklistDecl {
            common: ConstructCommon { name, params, ports, asserts, span: start.merge(closing.span) },
            kind: kind.unwrap_or(LinklistKind::Singly),
            track_tail,
            track_length,
            ops,
        })
    }

    fn parse_op_decl(&mut self) -> Result<OpDecl, CompileError> {
        let start = self.expect(TokenKind::Op)?.span;
        let name = self.expect_ident()?;

        let mut latency: u32 = 1;
        let mut pipelined = false;
        let mut ports = Vec::new();
        let mut asserts: Vec<AssertDecl> = Vec::new();

        while !self.check_end_op() {
            match self.peek_kind() {
                Some(TokenKind::Latency) => {
                    self.advance(); // consume 'latency'
                    self.expect(TokenKind::Colon)?;
                    match self.peek_kind() {
                        Some(TokenKind::DecLiteral(ref s)) => {
                            let s = s.clone();
                            latency = s.parse::<u32>().unwrap_or(1);
                            self.advance();
                        }
                        other => return Err(CompileError::unexpected_token(
                            "integer literal", &other.map(|k| k.to_string()).unwrap_or("EOF".into()), self.peek_span(),
                        )),
                    }
                    self.eat(TokenKind::Semi);
                }
                Some(TokenKind::Pipelined) => {
                    self.advance(); // consume 'pipelined'
                    self.expect(TokenKind::Colon)?;
                    pipelined = match self.peek_kind() {
                        Some(TokenKind::True)  => { self.advance(); true }
                        Some(TokenKind::False) => { self.advance(); false }
                        Some(TokenKind::Ident(ref s)) if s == "true"  => { self.advance(); true }
                        Some(TokenKind::Ident(ref s)) if s == "false" => { self.advance(); false }
                        other => return Err(CompileError::unexpected_token(
                            "true or false",
                            &other.map(|k| k.to_string()).unwrap_or("EOF".into()),
                            self.peek_span(),
                        )),
                    };
                    self.eat(TokenKind::Semi);
                }
                Some(TokenKind::Port) => ports.push(self.parse_port_decl()?),
                Some(TokenKind::Assert) | Some(TokenKind::Cover) => {
                    asserts.push(self.parse_assert_decl()?);
                }
                Some(other) => return Err(CompileError::unexpected_token(
                    "latency, pipelined, port, assert, or cover", &other.to_string(), self.peek_span(),
                )),
                None => return Err(CompileError::UnexpectedEof),
            }
        }

        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Op)?;
        let closing = self.expect_ident()?;
        if closing.name != name.name {
            return Err(CompileError::mismatched_closing(&name.name, &closing.name, closing.span));
        }

        Ok(OpDecl {
            common: ConstructCommon { name, params: Vec::new(), ports, asserts, span: start.merge(closing.span) },
            latency,
            pipelined,
        })
    }

    fn check_end_linklist(&self) -> bool {
        self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos].kind == TokenKind::End
            && self.tokens[self.pos + 1].kind == TokenKind::Linklist
    }

    fn check_end_op(&self) -> bool {
        self.pos + 1 < self.tokens.len()
            && self.tokens[self.pos].kind == TokenKind::End
            && self.tokens[self.pos + 1].kind == TokenKind::Op
    }
}

fn is_method_name(name: &str) -> bool {
    matches!(name, "trunc" | "zext" | "sext" | "resize" | "reverse")
}


fn prefix_bp() -> u8 {
    21 // unary prefix is highest
}

fn infix_binding_power(op: BinOp) -> (u8, u8) {
    match op {
        // `implies` has the lowest precedence; right-associative so (0,0) makes
        // `a implies b implies c` parse as `a implies (b implies c)`.
        BinOp::Implies => (0, 0),
        BinOp::Or  => (1, 2),
        BinOp::And => (3, 4),
        BinOp::Eq | BinOp::Neq => (5, 6),
        BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte => (7, 8),
        BinOp::BitOr  => (9, 10),
        BinOp::BitXor => (11, 12),
        BinOp::BitAnd => (13, 14),
        BinOp::Shl | BinOp::Shr => (15, 16),
        BinOp::Add | BinOp::Sub | BinOp::AddWrap | BinOp::SubWrap => (17, 18),
        BinOp::Mul | BinOp::Div | BinOp::Mod | BinOp::MulWrap => (19, 20),
    }
}

impl Parser {
    // ── Function ──────────────────────────────────────────────────────────────

    fn parse_function(&mut self) -> Result<FunctionDecl, CompileError> {
        let start = self.expect(TokenKind::Function)?.span;
        let name = self.expect_ident()?;

        // Arg list: (name: Type, ...)
        self.expect(TokenKind::LParen)?;
        let mut args = Vec::new();
        while !self.check(TokenKind::RParen) {
            let arg_name = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            let ty = self.parse_type_expr()?;
            args.push(FunctionArg { name: arg_name, ty });
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RParen)?;

        // Return type: ->
        self.expect(TokenKind::RArrow)?;
        let ret_ty = self.parse_type_expr()?;

        // Body: let, return, if/elsif/else, for, or assignment
        let body = self.parse_function_body()?;

        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Function)?;
        let closing_name = self.expect_ident()?;
        if closing_name.name != name.name {
            return Err(CompileError::mismatched_closing(
                &name.name,
                &closing_name.name,
                closing_name.span,
            ));
        }

        Ok(FunctionDecl {
            span: start.merge(closing_name.span),
            name,
            args,
            ret_ty,
            body,
        })
    }

    // --- Use ---
    fn parse_use(&mut self) -> Result<UseDecl, CompileError> {
        let start = self.expect(TokenKind::Use)?.span;
        let name = self.expect_ident()?;
        let end = self.expect(TokenKind::Semi)?.span;
        Ok(UseDecl {
            span: start.merge(end),
            name,
        })
    }

    /// Parse a function body: a sequence of let, return, if/elsif/else, for, or assignment statements.
    fn parse_function_body(&mut self) -> Result<Vec<FunctionBodyItem>, CompileError> {
        let mut body = Vec::new();
        while !self.check_end_keyword() && !self.check(TokenKind::Else) && !self.check(TokenKind::ElsIf) {
            if self.check(TokenKind::Let) {
                body.push(FunctionBodyItem::Let(self.parse_let_binding()?));
            } else if self.check(TokenKind::Return) {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(TokenKind::Semi)?;
                body.push(FunctionBodyItem::Return(expr));
            } else if self.check(TokenKind::If) {
                body.push(FunctionBodyItem::IfElse(self.parse_function_if()?));
            } else if self.check(TokenKind::For) {
                body.push(FunctionBodyItem::For(self.parse_function_for()?));
            } else {
                // Try parsing as assignment: expr = expr;
                let start_span = self.peek_span();
                let target = self.parse_expr()?;
                self.expect(TokenKind::Eq)?;
                let value = self.parse_expr()?;
                let end = self.expect(TokenKind::Semi)?;
                body.push(FunctionBodyItem::Assign(FunctionAssign {
                    target,
                    value,
                    span: start_span.merge(end.span),
                }));
            }
        }
        Ok(body)
    }

    /// Parse if/elsif/else inside a function body.
    fn parse_function_if(&mut self) -> Result<FunctionIfElse, CompileError> {
        let start = self.expect(TokenKind::If)?.span;
        let cond = self.parse_expr()?;
        let then_body = self.parse_function_body()?;

        let else_body = if self.check(TokenKind::ElsIf) {
            // elsif → parse as nested if
            vec![FunctionBodyItem::IfElse(self.parse_function_elsif()?)]
        } else if self.check(TokenKind::Else) {
            self.advance();
            self.parse_function_body()?
        } else {
            Vec::new()
        };

        let end = self.expect(TokenKind::End)?;
        self.expect(TokenKind::If)?;

        Ok(FunctionIfElse {
            cond,
            then_body,
            else_body,
            span: start.merge(end.span),
        })
    }

    /// Parse elsif branch (reuses function_if logic without consuming `if` keyword).
    fn parse_function_elsif(&mut self) -> Result<FunctionIfElse, CompileError> {
        let start = self.expect(TokenKind::ElsIf)?.span;
        let cond = self.parse_expr()?;
        let then_body = self.parse_function_body()?;

        let else_body = if self.check(TokenKind::ElsIf) {
            vec![FunctionBodyItem::IfElse(self.parse_function_elsif()?)]
        } else if self.check(TokenKind::Else) {
            self.advance();
            self.parse_function_body()?
        } else {
            Vec::new()
        };

        // elsif doesn't have its own `end if` — the outer `end if` closes the whole chain
        Ok(FunctionIfElse {
            cond,
            then_body,
            else_body,
            span: start,
        })
    }

    /// Parse for loop inside a function body.
    fn parse_function_for(&mut self) -> Result<FunctionFor, CompileError> {
        let start = self.expect(TokenKind::For)?.span;
        let var = self.expect_ident()?;
        self.expect_contextual("in")?;

        let range = if self.check(TokenKind::LBrace) {
            self.advance();
            let mut values = Vec::new();
            loop {
                values.push(self.parse_expr()?);
                if !self.eat(TokenKind::Comma) { break; }
            }
            self.expect(TokenKind::RBrace)?;
            ForRange::ValueList(values)
        } else {
            let range_start = self.parse_expr()?;
            self.expect(TokenKind::DotDot)?;
            let range_end = self.parse_expr()?;
            ForRange::Range(range_start, range_end)
        };

        let body = self.parse_function_body()?;
        let end = self.expect(TokenKind::End)?;
        self.expect(TokenKind::For)?;

        Ok(FunctionFor {
            var,
            range,
            body,
            span: start.merge(end.span),
        })
    }

    // --- Package ---
    fn parse_package(&mut self) -> Result<PackageDecl, CompileError> {
        let start = self.expect(TokenKind::Package)?.span;
        let name = self.expect_ident()?;
        let mut params = Vec::new();
        let mut domains = Vec::new();
        let mut enums = Vec::new();
        let mut structs = Vec::new();
        let mut functions = Vec::new();

        while !self.check_end_keyword() {
            match self.peek_kind() {
                _ if self.check_param() => params.push(self.parse_param_decl()?),
                Some(TokenKind::Domain) => domains.push(self.parse_domain()?),
                Some(TokenKind::Enum) => enums.push(self.parse_enum()?),
                Some(TokenKind::Struct) => structs.push(self.parse_struct()?),
                Some(TokenKind::Function) => functions.push(self.parse_function()?),
                Some(other) => {
                    return Err(CompileError::unexpected_token(
                        "param, domain, enum, struct, or function",
                        &other.to_string(),
                        self.peek_span(),
                    ));
                }
                None => return Err(CompileError::UnexpectedEof),
            }
        }

        self.expect(TokenKind::End)?;
        self.expect(TokenKind::Package)?;
        let closing_name = self.expect_ident()?;
        if closing_name.name != name.name {
            return Err(CompileError::mismatched_closing(
                &name.name,
                &closing_name.name,
                closing_name.span,
            ));
        }

        Ok(PackageDecl {
            span: start.merge(closing_name.span),
            name,
            params,
            domains,
            enums,
            structs,
            functions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;

    fn parse(src: &str) -> SourceFile {
        let tokens = tokenize(src).unwrap();
        let mut parser = Parser::new(tokens, src);
        parser.parse_source_file().unwrap()
    }

    #[test]
    fn test_parse_domain() {
        let sf = parse("domain SysDomain\n  freq_mhz: 100\nend domain SysDomain");
        assert_eq!(sf.items.len(), 1);
        match &sf.items[0] {
            Item::Domain(d) => {
                assert_eq!(d.name.name, "SysDomain");
                assert_eq!(d.fields.len(), 1);
            }
            _ => panic!("expected domain"),
        }
    }

    #[test]
    fn test_parse_struct() {
        let sf = parse("struct MyStruct\n  x: UInt<8>;\n  y: Bool;\nend struct MyStruct");
        match &sf.items[0] {
            Item::Struct(s) => {
                assert_eq!(s.name.name, "MyStruct");
                assert_eq!(s.fields.len(), 2);
            }
            _ => panic!("expected struct"),
        }
    }

    #[test]
    fn test_parse_enum() {
        let sf = parse("enum Color\n  Red,\n  Green,\n  Blue\nend enum Color");
        match &sf.items[0] {
            Item::Enum(e) => {
                assert_eq!(e.name.name, "Color");
                assert_eq!(e.variants.len(), 3);
            }
            _ => panic!("expected enum"),
        }
    }

    #[test]
    fn test_parse_simple_module() {
        let sf = parse(
            "module Counter\n\
             param WIDTH: const = 8;\n\
             port clk: in Clock<SysDomain>;\n\
             port count: out UInt<WIDTH>;\n\
             end module Counter",
        );
        match &sf.items[0] {
            Item::Module(m) => {
                assert_eq!(m.name.name, "Counter");
                assert_eq!(m.params.len(), 1);
                assert_eq!(m.ports.len(), 2);
            }
            _ => panic!("expected module"),
        }
    }

    #[test]
    fn test_mismatched_closing() {
        let src = "module Foo\nend module Bar";
        let tokens = tokenize(src).unwrap();
        let mut parser = Parser::new(tokens, src);
        assert!(parser.parse_source_file().is_err());
    }

    #[test]
    fn test_parse_expr_arithmetic() {
        let src = "module M\n  let x: UInt<8> = a + b * c;\nend module M";
        let tokens = tokenize(src).unwrap();
        let mut parser = Parser::new(tokens, src);
        let sf = parser.parse_source_file().unwrap();
        match &sf.items[0] {
            Item::Module(m) => assert_eq!(m.body.len(), 1),
            _ => panic!("expected module"),
        }
    }
}
