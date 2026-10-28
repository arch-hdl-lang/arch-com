//! Reader for the SystemVerilog subset that `arch build` emits for modules the
//! codegen certificate covers.
//!
//! This reader is part of the certificate's trusted base: it turns the
//! *emitted SV text* back into the `Sv.Module` data the Lean validator
//! reasons about. It deliberately shares no code with `codegen`, so a codegen
//! bug cannot be mirrored by the reader. Anything outside the subset is
//! rejected with a diagnostic rather than approximated.

use super::{BinOp, CmpOp, Dir, Port, SigTable, SvCw, SvExpr, SvModule, SvProc, SvStmt};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Num(u64),
    /// Based or sized literal: `8'hFF`, `'h5A`, `1'b0`, `32'sd5`.
    Lit {
        size: Option<u32>,
        signed: bool,
        value: u64,
    },
    /// The `'` of a size cast: `9'(...)`, `(expr)'(...)`.
    CastTick,
    Punct(&'static str),
}

const PUNCTS: &[&str] = &[
    "&&", "||", "==", "!=", "<=", ">=", "+:", "-:", "(", ")", "[", "]", ":", ";", ",", "=", "<",
    ">", "+", "-", "*", "/", "%", "&", "|", "^", "~", "!", "?", "@", "#",
];

fn lex(src: &str) -> Result<Vec<(Tok, usize)>, String> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut line = 1;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i] as char;
        if c == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if src[i..].starts_with("//") {
            let text: String = src[i..].chars().take_while(|&ch| ch != '\n').collect();
            if text.contains("translate_off") || text.contains("translate_on") {
                return Err(format!(
                    "line {line}: `{}` — tool-directive regions are outside the certified subset",
                    text.trim()
                ));
            }
            i += text.len();
            continue;
        }
        if src[i..].starts_with("/*") {
            let end = src[i + 2..]
                .find("*/")
                .ok_or_else(|| format!("line {line}: unterminated block comment"))?;
            line += src[i..i + 2 + end].matches('\n').count();
            i += end + 4;
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' || c == '$' {
            let start = i;
            while i < b.len()
                && ((b[i] as char).is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'$')
            {
                i += 1;
            }
            out.push((Tok::Ident(src[start..i].to_string()), line));
            continue;
        }
        if c.is_ascii_digit() {
            let start = i;
            while i < b.len() && ((b[i] as char).is_ascii_digit() || b[i] == b'_') {
                i += 1;
            }
            let n = parse_digits(&src[start..i], 10, line)?;
            if i < b.len() && b[i] == b'\'' {
                if i + 1 < b.len() && b[i + 1] == b'(' {
                    out.push((Tok::Num(n), line));
                    out.push((Tok::CastTick, line));
                    i += 1;
                    continue;
                }
                let size =
                    u32::try_from(n).map_err(|_| format!("line {line}: literal size too large"))?;
                let (tok, len) = based_literal(&src[i..], Some(size), line)?;
                out.push((tok, line));
                i += len;
                continue;
            }
            out.push((Tok::Num(n), line));
            continue;
        }
        if c == '\'' {
            if i + 1 < b.len() && b[i + 1] == b'(' {
                out.push((Tok::CastTick, line));
                i += 1;
                continue;
            }
            let (tok, len) = based_literal(&src[i..], None, line)?;
            out.push((tok, line));
            i += len;
            continue;
        }
        let Some(p) = PUNCTS.iter().find(|p| src[i..].starts_with(**p)) else {
            return Err(format!("line {line}: unexpected character `{c}`"));
        };
        out.push((Tok::Punct(p), line));
        i += p.len();
    }
    Ok(out)
}

fn parse_digits(text: &str, radix: u32, line: usize) -> Result<u64, String> {
    let clean: String = text.chars().filter(|&ch| ch != '_').collect();
    u64::from_str_radix(&clean, radix)
        .map_err(|_| format!("line {line}: unsupported literal digits `{text}`"))
}

/// Parse `'[s]{d,h,b,o}digits` starting at the tick. Returns the token and
/// the number of bytes consumed.
fn based_literal(src: &str, size: Option<u32>, line: usize) -> Result<(Tok, usize), String> {
    let b = src.as_bytes();
    let mut i = 1;
    let mut signed = false;
    if i < b.len() && (b[i] == b's' || b[i] == b'S') {
        signed = true;
        i += 1;
    }
    let radix = match b.get(i).map(|c| c.to_ascii_lowercase()) {
        Some(b'd') => 10,
        Some(b'h') => 16,
        Some(b'b') => 2,
        Some(b'o') => 8,
        _ => {
            return Err(format!(
                "line {line}: unsupported literal form starting `{}`",
                &src[..src.len().min(6)]
            ))
        }
    };
    i += 1;
    let start = i;
    while i < b.len() && ((b[i] as char).is_ascii_alphanumeric() || b[i] == b'_') {
        i += 1;
    }
    let digits = &src[start..i];
    if digits
        .chars()
        .any(|ch| matches!(ch.to_ascii_lowercase(), 'x' | 'z' | '?'))
    {
        return Err(format!(
            "line {line}: 4-state literal `{digits}` is outside the subset"
        ));
    }
    let value = parse_digits(digits, radix, line)?;
    Ok((
        Tok::Lit {
            size,
            signed,
            value,
        },
        i,
    ))
}

/// Parse tree before deciding whether a parenthesized expression is a
/// constant cast width or a value expression.
#[derive(Debug, Clone)]
enum P {
    Ident(String),
    Num(u64),
    Lit(Option<u32>, bool, u64),
    Bits(String),
    Unsigned(Box<P>),
    Bin(&'static str, Box<P>, Box<P>),
    Un(&'static str, Box<P>),
    Cond(Box<P>, Box<P>, Box<P>),
    Cast(Box<P>, Box<P>),
    Slice(String, u64, u64),
    Clog2(Box<P>),
}

struct Parser<'a> {
    toks: Vec<(Tok, usize)>,
    pos: usize,
    sigs: &'a SigTable,
    /// `parameter int` / `localparam int` values, evaluated from the SV text
    /// itself (not taken from the ARCH side).
    params: HashMap<String, u64>,
}

type R<T> = Result<T, String>;

/// `always_ff` clock, optional async `(signal, posedge?)` entry, and body.
type AlwaysFf = (u32, Option<(u32, bool)>, Vec<SvStmt>);

impl<'a> Parser<'a> {
    fn line(&self) -> usize {
        self.toks
            .get(self.pos)
            .or_else(|| self.toks.last())
            .map_or(0, |t| t.1)
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.0)
    }

    fn err<T>(&self, msg: &str) -> R<T> {
        let found = self
            .peek()
            .map_or("end of input".to_string(), |t| format!("{t:?}"));
        Err(format!("line {}: {msg} (found {found})", self.line()))
    }

    fn bump(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).map(|t| t.0.clone());
        self.pos += 1;
        t
    }

    fn is_punct(&self, p: &str) -> bool {
        matches!(self.peek(), Some(Tok::Punct(q)) if *q == p)
    }

    fn is_kw(&self, kw: &str) -> bool {
        matches!(self.peek(), Some(Tok::Ident(s)) if s == kw)
    }

    fn expect_punct(&mut self, p: &str) -> R<()> {
        if self.is_punct(p) {
            self.pos += 1;
            Ok(())
        } else {
            self.err(&format!("expected `{p}`"))
        }
    }

    fn expect_kw(&mut self, kw: &str) -> R<()> {
        if self.is_kw(kw) {
            self.pos += 1;
            Ok(())
        } else {
            self.err(&format!("expected `{kw}`"))
        }
    }

    fn ident(&mut self) -> R<String> {
        match self.peek() {
            Some(Tok::Ident(s)) if !s.starts_with('$') => {
                let s = s.clone();
                self.pos += 1;
                Ok(s)
            }
            _ => self.err("expected identifier"),
        }
    }

    fn sig(&self, name: &str) -> R<u32> {
        self.sigs.get(name).ok_or_else(|| {
            format!(
                "line {}: SV references `{name}`, which is not a signal of the ARCH module",
                self.line()
            )
        })
    }

    /// `[hi:0]` packed range, or nothing (1 bit).
    fn packed_width(&mut self) -> R<u32> {
        if !self.is_punct("[") {
            return Ok(1);
        }
        self.pos += 1;
        let hi = self.const_expr()?;
        self.expect_punct(":")?;
        let lo = self.const_expr()?;
        self.expect_punct("]")?;
        if lo != 0 {
            return self.err("only `[N:0]` packed ranges are supported");
        }
        u32::try_from(hi + 1).map_err(|_| "packed width too large".to_string())
    }

    // ── expressions ────────────────────────────────────────────────────

    fn expr(&mut self) -> R<P> {
        let c = self.binary(0)?;
        if self.is_punct("?") {
            self.pos += 1;
            let t = self.expr()?;
            self.expect_punct(":")?;
            let f = self.expr()?;
            return Ok(P::Cond(Box::new(c), Box::new(t), Box::new(f)));
        }
        Ok(c)
    }

    /// Binary operators by IEEE 1800 Table 11-2 precedence, loosest first.
    fn binary(&mut self, level: usize) -> R<P> {
        const LEVELS: &[&[&str]] = &[
            &["||"],
            &["&&"],
            &["|"],
            &["^"],
            &["&"],
            &["==", "!="],
            &["<", "<=", ">", ">="],
            &["+", "-"],
            &["*", "/", "%"],
        ];
        if level == LEVELS.len() {
            return self.unary();
        }
        let mut lhs = self.binary(level + 1)?;
        loop {
            let op = match self.peek() {
                Some(Tok::Punct(p)) if LEVELS[level].contains(p) => *p,
                _ => break,
            };
            self.pos += 1;
            let rhs = self.binary(level + 1)?;
            lhs = P::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> R<P> {
        for op in ["!", "~"] {
            if self.is_punct(op) {
                self.pos += 1;
                let a = self.unary()?;
                return Ok(P::Un(op, Box::new(a)));
            }
        }
        self.primary()
    }

    fn primary(&mut self) -> R<P> {
        match self.bump() {
            Some(Tok::Num(n)) => {
                if matches!(self.peek(), Some(Tok::CastTick)) {
                    self.pos += 1;
                    let e = self.paren_expr()?;
                    return Ok(P::Cast(Box::new(P::Num(n)), Box::new(e)));
                }
                Ok(P::Num(n))
            }
            Some(Tok::Lit {
                size,
                signed,
                value,
            }) => Ok(P::Lit(size, signed, value)),
            Some(Tok::Punct("(")) => {
                let e = self.expr()?;
                self.expect_punct(")")?;
                if matches!(self.peek(), Some(Tok::CastTick)) {
                    self.pos += 1;
                    let inner = self.paren_expr()?;
                    return Ok(P::Cast(Box::new(e), Box::new(inner)));
                }
                Ok(e)
            }
            Some(Tok::Ident(name)) if name == "$bits" => {
                self.expect_punct("(")?;
                let x = self.ident()?;
                self.expect_punct(")")?;
                Ok(P::Bits(x))
            }
            Some(Tok::Ident(name)) if name == "$unsigned" => {
                let e = self.paren_expr()?;
                Ok(P::Unsigned(Box::new(e)))
            }
            Some(Tok::Ident(name)) if name == "$clog2" => {
                let e = self.paren_expr()?;
                Ok(P::Clog2(Box::new(e)))
            }
            Some(Tok::Ident(name)) if !name.starts_with('$') => {
                if self.is_punct("[") {
                    // Constant selects only: `x[hi:lo]`, `x[base +: w]`, `x[base -: w]`.
                    self.pos += 1;
                    let first = self.const_expr()?;
                    let (hi, lo) = if self.is_punct(":") {
                        self.pos += 1;
                        (first, self.const_expr()?)
                    } else if self.is_punct("+:") || self.is_punct("-:") {
                        let up = self.is_punct("+:");
                        self.pos += 1;
                        let w = self.const_expr()?;
                        if w == 0 {
                            return self.err("zero-width part-select");
                        }
                        if up {
                            (first + w - 1, first)
                        } else {
                            let lo = first.checked_sub(w - 1).ok_or_else(|| {
                                format!("line {}: part-select below bit 0", self.line())
                            })?;
                            (first, lo)
                        }
                    } else {
                        return self.err("expected `:`, `+:` or `-:` in a select");
                    };
                    self.expect_punct("]")?;
                    return Ok(P::Slice(name, hi, lo));
                }
                if matches!(self.peek(), Some(Tok::CastTick)) {
                    // `W'(e)`: a parameter as the cast width.
                    self.pos += 1;
                    let e = self.paren_expr()?;
                    return Ok(P::Cast(Box::new(P::Ident(name)), Box::new(e)));
                }
                Ok(P::Ident(name))
            }
            _ => {
                self.pos -= 1;
                self.err("unsupported expression")
            }
        }
    }

    fn paren_expr(&mut self) -> R<P> {
        self.expect_punct("(")?;
        let e = self.expr()?;
        self.expect_punct(")")?;
        Ok(e)
    }

    /// Parse and evaluate a constant integer expression.
    fn const_expr(&mut self) -> R<u64> {
        let p = self.expr()?;
        self.ceval(&p)
            .map_err(|e| format!("line {}: {e}", self.line()))
    }

    /// Evaluate a constant integer expression over the parameters read so far.
    /// Results stay within the non-negative `int` range.
    fn ceval(&self, p: &P) -> R<u64> {
        const LIMIT: u64 = 1 << 31;
        let v = match p {
            P::Num(n) => *n,
            P::Ident(x) => *self
                .params
                .get(x)
                .ok_or_else(|| format!("`{x}` is not a constant"))?,
            P::Bin(op, a, b) => {
                let (a, b) = (self.ceval(a)?, self.ceval(b)?);
                match *op {
                    "+" => a + b,
                    "-" => a
                        .checked_sub(b)
                        .ok_or_else(|| "negative constant expression".to_string())?,
                    "*" => a
                        .checked_mul(b)
                        .ok_or_else(|| "constant expression overflows".to_string())?,
                    "/" | "%" if b == 0 => return Err("division by zero".to_string()),
                    "/" => a / b,
                    "%" => a % b,
                    other => return Err(format!("operator `{other}` in a constant expression")),
                }
            }
            P::Clog2(a) => {
                let a = self.ceval(a)?;
                if a <= 1 {
                    0
                } else {
                    u64::from(64 - (a - 1).leading_zeros())
                }
            }
            _ => return Err(format!("unsupported constant expression {p:?}")),
        };
        if v >= LIMIT {
            return Err("constant exceeds the 32-bit `int` range".to_string());
        }
        Ok(v)
    }

    fn to_cw(&self, p: &P) -> R<SvCw> {
        Ok(match p {
            P::Num(n) => SvCw::Num(*n),
            P::Bits(x) => SvCw::Bits(self.sig(x)?),
            P::Cond(c, t, f) => match &**c {
                P::Bin(op, a, b) => {
                    let op = cmp_op(op)
                        .ok_or_else(|| format!("unsupported cast-width comparison `{op}`"))?;
                    SvCw::Sel(
                        op,
                        Box::new(self.to_cw(a)?),
                        Box::new(self.to_cw(b)?),
                        Box::new(self.to_cw(t)?),
                        Box::new(self.to_cw(f)?),
                    )
                }
                _ => return Err(format!("unsupported cast-width condition {c:?}")),
            },
            P::Ident(_) | P::Bin(..) | P::Clog2(_) => SvCw::Num(self.ceval(p)?),
            _ => return Err(format!("unsupported cast width {p:?}")),
        })
    }

    fn to_expr(&self, p: &P) -> R<SvExpr> {
        Ok(match p {
            // A `parameter int` is a 32-bit signed constant (IEEE 1800 §6.20.2).
            P::Ident(x) if self.params.contains_key(x) => SvExpr::Lit(None, true, self.params[x]),
            P::Ident(x) => SvExpr::Ident(self.sig(x)?),
            P::Clog2(_) => SvExpr::Lit(None, true, self.ceval(p)?),
            P::Num(n) => SvExpr::Lit(None, true, *n),
            P::Lit(size, signed, value) => {
                if *size == Some(0) {
                    return Err("zero-width literal".to_string());
                }
                SvExpr::Lit(*size, *signed, *value)
            }
            P::Bits(_) => return Err("`$bits` outside a cast width".to_string()),
            P::Unsigned(e) => SvExpr::Unsigned(Box::new(self.to_expr(e)?)),
            P::Bin(op, a, b) => {
                let (a, b) = (Box::new(self.to_expr(a)?), Box::new(self.to_expr(b)?));
                match *op {
                    "&&" => SvExpr::Land(a, b),
                    "||" => SvExpr::Lor(a, b),
                    other => {
                        if let Some(c) = cmp_op(other) {
                            SvExpr::Cmp(c, a, b)
                        } else {
                            let bop = match other {
                                "+" => BinOp::Add,
                                "-" => BinOp::Sub,
                                "*" => BinOp::Mul,
                                "&" => BinOp::And,
                                "|" => BinOp::Or,
                                "^" => BinOp::Xor,
                                _ => return Err(format!("unsupported operator `{other}`")),
                            };
                            SvExpr::Bin(bop, a, b)
                        }
                    }
                }
            }
            P::Un("!", a) => SvExpr::Lnot(Box::new(self.to_expr(a)?)),
            P::Un(_, a) => SvExpr::Bnot(Box::new(self.to_expr(a)?)),
            P::Cond(c, t, f) => SvExpr::Cond(
                Box::new(self.to_expr(c)?),
                Box::new(self.to_expr(t)?),
                Box::new(self.to_expr(f)?),
            ),
            P::Cast(w, e) => SvExpr::Cast(self.to_cw(w)?, Box::new(self.to_expr(e)?)),
            P::Slice(x, hi, lo) => {
                if lo > hi {
                    return Err(format!("descending slice `{x}[{hi}:{lo}]`"));
                }
                SvExpr::Slice(self.sig(x)?, *hi, *lo)
            }
        })
    }

    fn value_expr(&mut self) -> R<SvExpr> {
        let p = self.expr()?;
        self.to_expr(&p)
            .map_err(|e| format!("line {}: {e}", self.line()))
    }

    // ── statements ─────────────────────────────────────────────────────

    /// `begin stmts end` or a single statement.
    fn block(&mut self, nonblocking: bool) -> R<Vec<SvStmt>> {
        if self.is_kw("begin") {
            self.pos += 1;
            let mut out = Vec::new();
            while !self.is_kw("end") {
                out.push(self.stmt(nonblocking)?);
            }
            self.pos += 1;
            Ok(out)
        } else {
            Ok(vec![self.stmt(nonblocking)?])
        }
    }

    fn stmt(&mut self, nonblocking: bool) -> R<SvStmt> {
        if self.is_kw("if") {
            self.pos += 1;
            let c = self.paren_expr()?;
            let c = self
                .to_expr(&c)
                .map_err(|e| format!("line {}: {e}", self.line()))?;
            let t = self.block(nonblocking)?;
            let f = if self.is_kw("else") {
                self.pos += 1;
                if self.is_kw("if") {
                    vec![self.stmt(nonblocking)?]
                } else {
                    self.block(nonblocking)?
                }
            } else {
                Vec::new()
            };
            return Ok(SvStmt::Ite(c, t, f));
        }
        let x = self.ident()?;
        let x = self.sig(&x)?;
        if nonblocking {
            self.expect_punct("<=")?;
        } else {
            self.expect_punct("=")?;
        }
        let e = self.value_expr()?;
        self.expect_punct(";")?;
        Ok(SvStmt::Assign(x, e))
    }

    // ── module ─────────────────────────────────────────────────────────

    /// `int NAME = const-expr` after `parameter` / `localparam`.
    fn param_decl(&mut self) -> R<()> {
        if !self.is_kw("int") {
            return self.err("only `int` parameters are supported");
        }
        self.pos += 1;
        let n = self.ident()?;
        if self.sigs.get(&n).is_some() || self.params.contains_key(&n) {
            return self.err(&format!("parameter `{n}` shadows another name"));
        }
        self.expect_punct("=")?;
        let v = self.const_expr()?;
        self.params.insert(n, v);
        Ok(())
    }

    fn module(&mut self, name: &str) -> R<SvModule> {
        self.expect_kw("module")?;
        let got = self.ident()?;
        if got != name {
            return self.err(&format!("expected module `{name}`"));
        }
        if self.is_punct("#") {
            self.pos += 1;
            self.expect_punct("(")?;
            loop {
                self.expect_kw("parameter")?;
                self.param_decl()?;
                if self.is_punct(",") {
                    self.pos += 1;
                    continue;
                }
                break;
            }
            self.expect_punct(")")?;
        }
        let mut ports = Vec::new();
        self.expect_punct("(")?;
        if !self.is_punct(")") {
            loop {
                let dir = if self.is_kw("input") {
                    Dir::Input
                } else if self.is_kw("output") {
                    Dir::Output
                } else {
                    return self.err("expected `input` or `output`");
                };
                self.pos += 1;
                self.expect_kw("logic")?;
                let width = self.packed_width()?;
                let n = self.ident()?;
                ports.push(Port {
                    sig: self.sig(&n)?,
                    dir,
                    width,
                });
                if self.is_punct(",") {
                    self.pos += 1;
                    continue;
                }
                break;
            }
        }
        self.expect_punct(")")?;
        self.expect_punct(";")?;

        let mut locals = Vec::new();
        let mut procs = Vec::new();
        let mut inits = Vec::new();
        let mut ff: Option<AlwaysFf> = None;
        loop {
            if self.is_kw("endmodule") {
                self.pos += 1;
                break;
            }
            if self.is_kw("localparam") {
                self.pos += 1;
                self.param_decl()?;
                self.expect_punct(";")?;
            } else if self.is_kw("logic") {
                self.pos += 1;
                let width = self.packed_width()?;
                let n = self.ident()?;
                let x = self.sig(&n)?;
                if self.is_punct("=") {
                    self.pos += 1;
                    let e = self.value_expr()?;
                    inits.push((x, e));
                }
                self.expect_punct(";")?;
                locals.push((x, width));
            } else if self.is_kw("assign") {
                self.pos += 1;
                let x = self.ident()?;
                let x = self.sig(&x)?;
                self.expect_punct("=")?;
                let e = self.value_expr()?;
                self.expect_punct(";")?;
                procs.push(SvProc::Assign(x, e));
            } else if self.is_kw("always_comb") {
                self.pos += 1;
                procs.push(SvProc::Comb(self.block(false)?));
            } else if self.is_kw("always_ff") {
                self.pos += 1;
                if ff.is_some() {
                    return self.err("more than one always_ff is outside the subset");
                }
                self.expect_punct("@")?;
                self.expect_punct("(")?;
                self.expect_kw("posedge")?;
                let clk = self.ident()?;
                let clk = self.sig(&clk)?;
                let mut asynch = None;
                if self.is_kw("or") {
                    self.pos += 1;
                    let pos = if self.is_kw("posedge") {
                        true
                    } else if self.is_kw("negedge") {
                        false
                    } else {
                        return self.err("expected `posedge` or `negedge`");
                    };
                    self.pos += 1;
                    let r = self.ident()?;
                    asynch = Some((self.sig(&r)?, pos));
                }
                if !self.is_punct(")") {
                    return self.err("unsupported `always_ff` sensitivity list");
                }
                self.pos += 1;
                ff = Some((clk, asynch, self.block(true)?));
            } else {
                return self.err("unsupported module item");
            }
        }
        let (ff_clk, ff_async, ff_body) = match ff {
            Some((c, a, b)) => (Some(c), a, b),
            None => (None, None, Vec::new()),
        };
        Ok(SvModule {
            ports,
            locals,
            procs,
            ff_clk,
            ff_async,
            ff_body,
            inits,
        })
    }
}

fn cmp_op(op: &str) -> Option<CmpOp> {
    Some(match op {
        "==" => CmpOp::Eq,
        "!=" => CmpOp::Ne,
        "<" => CmpOp::Lt,
        "<=" => CmpOp::Le,
        ">" => CmpOp::Gt,
        ">=" => CmpOp::Ge,
        _ => return None,
    })
}

/// Read module `name` out of the full emitted SV text. Only that module's
/// text is lexed, so constructs elsewhere in the file do not matter.
pub(super) fn read_module(sv_text: &str, name: &str, sigs: &SigTable) -> R<SvModule> {
    let header = format!("module {name}");
    let start = sv_text
        .match_indices(&header)
        .map(|(i, _)| i)
        .find(|&i| {
            let at_line_start = i == 0 || sv_text.as_bytes()[i - 1] == b'\n';
            let next = sv_text[i + header.len()..].chars().next();
            at_line_start && matches!(next, Some(c) if c.is_whitespace() || c == '(' || c == '#')
        })
        .ok_or_else(|| format!("module `{name}` not found in emitted SV"))?;
    let end = sv_text[start..]
        .find("endmodule")
        .map(|i| start + i + "endmodule".len())
        .ok_or_else(|| format!("module `{name}` has no `endmodule`"))?;
    let mut p = Parser {
        toks: lex(&sv_text[start..end])?,
        pos: 0,
        sigs,
        params: HashMap::new(),
    };
    p.module(name)
}
