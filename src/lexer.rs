use logos::Logos;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\r\n\f]+")]
#[logos(skip r"//[^\n]*")]
#[logos(skip r"/\*([^*]|\*[^/])*\*/")]
pub enum TokenKind {
    // Keywords
    #[token("module")]
    Module,
    #[token("end")]
    End,
    #[token("param")]
    Param,
    #[token("port")]
    Port,
    #[token("in")]
    In,
    #[token("out")]
    Out,
    #[token("comb")]
    Comb,
    #[token("reg")]
    Reg,
    #[token("on")]
    On,
    #[token("rising")]
    Rising,
    #[token("falling")]
    Falling,
    #[token("high")]
    High,
    #[token("low")]
    Low,
    #[token("init")]
    Init,
    #[token("if")]
    If,
    #[token("else")]
    Else,
    #[token("match")]
    Match,
    #[token("let")]
    Let,
    #[token("inst")]
    Inst,
    #[token("connect")]
    Connect,
    #[token("struct")]
    Struct,
    #[token("enum")]
    Enum,
    #[token("domain")]
    Domain,
    #[token("fsm")]
    Fsm,
    #[token("fifo")]
    Fifo,
    #[token("ram")]
    Ram,
    #[token("store")]
    Store,
    #[token("counter")]
    Counter,
    #[token("arbiter")]
    Arbiter,
    #[token("regfile")]
    Regfile,
    #[token("ports")]
    Ports,
    #[token("forward")]
    Forward,
    #[token("state")]
    State,
    #[token("default")]
    Default,
    #[token("transition")]
    Transition,
    #[token("to")]
    To,
    #[token("when")]
    When,
    #[token("todo!")]
    Todo,
    #[token("const")]
    Const,
    #[token("type")]
    Type,
    #[token("as")]
    As,
    #[token("and")]
    And,
    #[token("or")]
    Or,
    #[token("not")]
    Not,
    #[token("true")]
    True,
    #[token("false")]
    False,
    #[token("assert")]
    Assert,
    #[token("cover")]
    Cover,

    // Type keywords
    #[token("UInt")]
    UInt,
    #[token("SInt")]
    SInt,
    #[token("Bool")]
    Bool,
    #[token("Bit")]
    Bit,
    #[token("Clock")]
    Clock,
    #[token("Reset")]
    Reset,
    #[token("Sync")]
    Sync,
    #[token("Async")]
    Async,
    #[token("Vec")]
    KwVec,

    // Operators and punctuation
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,
    #[token("==")]
    EqEq,
    #[token("!=")]
    BangEq,
    #[token("<=")]
    LtEq,
    #[token(">=")]
    GtEq,
    #[token("<-")]
    LArrow,
    #[token("->")]
    RArrow,
    #[token("=>")]
    FatArrow,
    #[token("<")]
    Lt,
    #[token(">")]
    Gt,
    #[token("=")]
    Eq,
    #[token("&")]
    Amp,
    #[token("|")]
    Pipe,
    #[token("^")]
    Caret,
    #[token("~")]
    Tilde,
    #[token("<<")]
    Shl,
    #[token(">>")]
    Shr,
    #[token("::")]
    ColonColon,
    #[token(".")]
    Dot,
    #[token(":")]
    Colon,
    #[token(";")]
    Semi,
    #[token(",")]
    Comma,
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token("!")]
    Bang,
    #[token("?")]
    Question,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token("_")]
    Underscore,

    // Literals
    #[regex(r"0x[0-9a-fA-F][0-9a-fA-F_]*", |lex| lex.slice().to_string())]
    HexLiteral(String),

    #[regex(r"0b[01][01_]*", |lex| lex.slice().to_string())]
    BinLiteral(String),

    #[regex(r"[0-9]+'[bhd][0-9a-fA-F_]+", |lex| lex.slice().to_string())]
    SizedLiteral(String),

    #[regex(r"[0-9][0-9_]*", priority = 2, callback = |lex| lex.slice().to_string())]
    DecLiteral(String),

    // Identifier
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", priority = 1, callback = |lex| lex.slice().to_string())]
    Ident(String),
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Module => write!(f, "module"),
            TokenKind::End => write!(f, "end"),
            TokenKind::Param => write!(f, "param"),
            TokenKind::Port => write!(f, "port"),
            TokenKind::In => write!(f, "in"),
            TokenKind::Out => write!(f, "out"),
            TokenKind::Comb => write!(f, "comb"),
            TokenKind::Reg => write!(f, "reg"),
            TokenKind::On => write!(f, "on"),
            TokenKind::Rising => write!(f, "rising"),
            TokenKind::Falling => write!(f, "falling"),
            TokenKind::High => write!(f, "high"),
            TokenKind::Low => write!(f, "low"),
            TokenKind::Init => write!(f, "init"),
            TokenKind::If => write!(f, "if"),
            TokenKind::Else => write!(f, "else"),
            TokenKind::Match => write!(f, "match"),
            TokenKind::Let => write!(f, "let"),
            TokenKind::Inst => write!(f, "inst"),
            TokenKind::Connect => write!(f, "connect"),
            TokenKind::Struct => write!(f, "struct"),
            TokenKind::Enum => write!(f, "enum"),
            TokenKind::Domain => write!(f, "domain"),
            TokenKind::Todo => write!(f, "todo!"),
            TokenKind::Const => write!(f, "const"),
            TokenKind::Type => write!(f, "type"),
            TokenKind::As => write!(f, "as"),
            TokenKind::And => write!(f, "and"),
            TokenKind::Or => write!(f, "or"),
            TokenKind::Not => write!(f, "not"),
            TokenKind::True => write!(f, "true"),
            TokenKind::False => write!(f, "false"),
            TokenKind::Assert => write!(f, "assert"),
            TokenKind::Cover => write!(f, "cover"),
            TokenKind::Fsm => write!(f, "fsm"),
            TokenKind::Fifo => write!(f, "fifo"),
            TokenKind::Ram => write!(f, "ram"),
            TokenKind::Store => write!(f, "store"),
            TokenKind::Counter => write!(f, "counter"),
            TokenKind::Arbiter => write!(f, "arbiter"),
            TokenKind::Regfile => write!(f, "regfile"),
            TokenKind::Ports => write!(f, "ports"),
            TokenKind::Forward => write!(f, "forward"),
            TokenKind::State => write!(f, "state"),
            TokenKind::Default => write!(f, "default"),
            TokenKind::Transition => write!(f, "transition"),
            TokenKind::To => write!(f, "to"),
            TokenKind::When => write!(f, "when"),
            TokenKind::UInt => write!(f, "UInt"),
            TokenKind::SInt => write!(f, "SInt"),
            TokenKind::Bool => write!(f, "Bool"),
            TokenKind::Bit => write!(f, "Bit"),
            TokenKind::Clock => write!(f, "Clock"),
            TokenKind::Reset => write!(f, "Reset"),
            TokenKind::Sync => write!(f, "Sync"),
            TokenKind::Async => write!(f, "Async"),
            TokenKind::KwVec => write!(f, "Vec"),
            TokenKind::Plus => write!(f, "+"),
            TokenKind::Minus => write!(f, "-"),
            TokenKind::Star => write!(f, "*"),
            TokenKind::Slash => write!(f, "/"),
            TokenKind::Percent => write!(f, "%"),
            TokenKind::EqEq => write!(f, "=="),
            TokenKind::BangEq => write!(f, "!="),
            TokenKind::LtEq => write!(f, "<="),
            TokenKind::GtEq => write!(f, ">="),
            TokenKind::LArrow => write!(f, "<-"),
            TokenKind::RArrow => write!(f, "->"),
            TokenKind::FatArrow => write!(f, "=>"),
            TokenKind::Lt => write!(f, "<"),
            TokenKind::Gt => write!(f, ">"),
            TokenKind::Eq => write!(f, "="),
            TokenKind::Amp => write!(f, "&"),
            TokenKind::Pipe => write!(f, "|"),
            TokenKind::Caret => write!(f, "^"),
            TokenKind::Tilde => write!(f, "~"),
            TokenKind::Shl => write!(f, "<<"),
            TokenKind::Shr => write!(f, ">>"),
            TokenKind::ColonColon => write!(f, "::"),
            TokenKind::Dot => write!(f, "."),
            TokenKind::Colon => write!(f, ":"),
            TokenKind::Semi => write!(f, ";"),
            TokenKind::Comma => write!(f, ","),
            TokenKind::LParen => write!(f, "("),
            TokenKind::RParen => write!(f, ")"),
            TokenKind::LBracket => write!(f, "["),
            TokenKind::RBracket => write!(f, "]"),
            TokenKind::Bang => write!(f, "!"),
            TokenKind::Question => write!(f, "?"),
            TokenKind::LBrace => write!(f, "{{"),
            TokenKind::RBrace => write!(f, "}}"),
            TokenKind::Underscore => write!(f, "_"),
            TokenKind::HexLiteral(s) => write!(f, "{s}"),
            TokenKind::BinLiteral(s) => write!(f, "{s}"),
            TokenKind::SizedLiteral(s) => write!(f, "{s}"),
            TokenKind::DecLiteral(s) => write!(f, "{s}"),
            TokenKind::Ident(s) => write!(f, "{s}"),
        }
    }
}

/// Extract all `//` and `/* */` comments from raw source text, returning their
/// byte spans and text. Does not rely on logos so it works independently of the
/// main tokenizer.
pub fn extract_comments(src: &str) -> Vec<(Span, String)> {
    let mut comments = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            // Line comment — consume until newline (do not include the '\n')
            let start = i;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            comments.push((Span::new(start, i), src[start..i].to_string()));
        } else if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            // Block comment — consume until '*/'
            let start = i;
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            if i + 1 < bytes.len() {
                i += 2; // consume the closing */
            }
            comments.push((Span::new(start, i), src[start..i].to_string()));
        } else if bytes[i] == b'"' {
            // Skip string literal so we don't mis-parse // or /* inside strings
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i += 1; // closing quote
        } else {
            i += 1;
        }
    }

    comments
}

pub fn tokenize(source: &str) -> Result<Vec<Token>, Vec<Span>> {
    let mut tokens = Vec::new();
    let mut errors = Vec::new();
    let mut lex = TokenKind::lexer(source);

    while let Some(result) = lex.next() {
        let span = lex.span();
        let span = Span::new(span.start, span.end);
        match result {
            Ok(kind) => tokens.push(Token { kind, span }),
            Err(()) => errors.push(span),
        }
    }

    if errors.is_empty() {
        Ok(tokens)
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_tokens() {
        let tokens = tokenize("module Counter end module Counter").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Module);
        assert_eq!(tokens[1].kind, TokenKind::Ident("Counter".into()));
        assert_eq!(tokens[2].kind, TokenKind::End);
        assert_eq!(tokens[3].kind, TokenKind::Module);
        assert_eq!(tokens[4].kind, TokenKind::Ident("Counter".into()));
    }

    #[test]
    fn test_operators() {
        let tokens = tokenize("+ - * == != <= >= <- -> => << >>").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Plus);
        assert_eq!(tokens[1].kind, TokenKind::Minus);
        assert_eq!(tokens[2].kind, TokenKind::Star);
        assert_eq!(tokens[3].kind, TokenKind::EqEq);
        assert_eq!(tokens[4].kind, TokenKind::BangEq);
        assert_eq!(tokens[5].kind, TokenKind::LtEq);
        assert_eq!(tokens[6].kind, TokenKind::GtEq);
        assert_eq!(tokens[7].kind, TokenKind::LArrow);
        assert_eq!(tokens[8].kind, TokenKind::RArrow);
        assert_eq!(tokens[9].kind, TokenKind::FatArrow);
        assert_eq!(tokens[10].kind, TokenKind::Shl);
        assert_eq!(tokens[11].kind, TokenKind::Shr);
    }

    #[test]
    fn test_literals() {
        let tokens = tokenize("42 0xFF 0b1010 8'hAB").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::DecLiteral("42".into()));
        assert_eq!(tokens[1].kind, TokenKind::HexLiteral("0xFF".into()));
        assert_eq!(tokens[2].kind, TokenKind::BinLiteral("0b1010".into()));
        assert_eq!(tokens[3].kind, TokenKind::SizedLiteral("8'hAB".into()));
    }

    #[test]
    fn test_type_keywords() {
        let tokens = tokenize("UInt SInt Bool Bit Clock Reset Sync Async Vec").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::UInt);
        assert_eq!(tokens[1].kind, TokenKind::SInt);
        assert_eq!(tokens[2].kind, TokenKind::Bool);
        assert_eq!(tokens[3].kind, TokenKind::Bit);
        assert_eq!(tokens[4].kind, TokenKind::Clock);
        assert_eq!(tokens[5].kind, TokenKind::Reset);
        assert_eq!(tokens[6].kind, TokenKind::Sync);
        assert_eq!(tokens[7].kind, TokenKind::Async);
        assert_eq!(tokens[8].kind, TokenKind::KwVec);
    }

    #[test]
    fn test_comments_skipped() {
        let tokens = tokenize("module // this is a comment\nCounter").unwrap();
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].kind, TokenKind::Module);
        assert_eq!(tokens[1].kind, TokenKind::Ident("Counter".into()));
    }

    #[test]
    fn test_todo() {
        let tokens = tokenize("todo!").unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Todo);
    }
}
