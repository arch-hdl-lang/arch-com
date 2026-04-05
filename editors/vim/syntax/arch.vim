" Vim syntax file for the ARCH HDL language
" Language:    ARCH HDL (arch-com compiler)
" Maintainer:  arch-com project
" File types:  *.arch

if exists("b:current_syntax")
  finish
endif

" ── Top-level construct keywords ─────────────────────────────────────────────
" These open a named block: `keyword Name ... end keyword Name`
syn keyword archConstruct  module pipeline fsm fifo ram arbiter regfile counter
syn keyword archConstruct  domain struct enum package bus template
syn keyword archConstruct  synchronizer clkgate linklist
syn keyword archConstruct  nextgroup=archConstructName skipwhite

" `end` followed by a construct keyword closes a block
syn keyword archEnd        end

" ── Block-structure keywords ──────────────────────────────────────────────────
" Appear inside construct bodies to declare sub-sections
syn keyword archBlock      port ports param socket state default
syn keyword archBlock      comb seq latch reg wire let inst generate
syn keyword archBlock      assert cover function return use
syn keyword archBlock      stage store hook implements
syn keyword archBlock      testbench initial repeat none

" ── Signal / control flow keywords ───────────────────────────────────────────
syn keyword archControl    if elsif else match unique when
syn keyword archControl    on rising falling high low
syn keyword archControl    await await_all await_any
syn keyword archControl    forward init reset for in from
syn keyword archControl    stall flush inside

" ── RAM / FIFO / counter attributes ──────────────────────────────────────────
syn keyword archAttr       kind read write mode direction policy
syn keyword archAttr       single simple_dual true_dual
syn keyword archAttr       sync async sync_out no_change
syn keyword archAttr       wrap saturate up down
syn keyword archAttr       round_robin priority weighted lru custom
syn keyword archAttr       blocking pipelined out_of_order burst
syn keyword archAttr       zero freq_mhz latency
syn keyword archAttr       write_before_read
syn keyword archAttr       pipe_reg op track pipelined

" ── Port direction ────────────────────────────────────────────────────────────
syn keyword archDir        in out initiator target
syn keyword archDir        nextgroup=archType skipwhite

" ── Built-in types ────────────────────────────────────────────────────────────
syn keyword archType       UInt SInt Bool Bit Clock Reset Vec Token Future
syn keyword archType       Sync Async

" ── Type / param meta-keywords ───────────────────────────────────────────────
syn keyword archParamKind  const type

" ── Boolean literals ─────────────────────────────────────────────────────────
syn keyword archBool       true false

" ── todo! escape hatch ────────────────────────────────────────────────────────
syn keyword archTodo       todo!

" ── Cast keywords ────────────────────────────────────────────────────────────
syn keyword archCast       as signed unsigned

" ── Built-in functions ──────────────────────────────────────────────────────
syn match   archBuiltinFn  /\$clog2/
syn keyword archBuiltinFn  log

" ── Boolean operators (word-form) ────────────────────────────────────────────
syn keyword archBoolOp     and or not

" ── Operators ────────────────────────────────────────────────────────────────
" Assignment / connection arrows / match fat arrow / scope
syn match   archOp         /<=\|<-\|->\|=>\|::/
" Arithmetic and comparison
syn match   archOp         /[+\-*\/&|^~%]/
syn match   archOp         /[=!<>]=\|[<>]/

" ── Numeric literals ─────────────────────────────────────────────────────────
" Sized Verilog-style: 8'd255  16'hFF  4'b1010
syn match   archSizedLit   /\<[0-9]\+'[bdh][0-9a-fA-F_xXzZ]\+\>/
" Hex: 0xFF
syn match   archHexLit     /\<0[xX][0-9a-fA-F_]\+\>/
" Binary: 0b1010
syn match   archBinLit     /\<0[bB][01_]\+\>/
" Plain decimal
syn match   archDecLit     /\<[0-9][0-9_]*\>/

" ── Width parameters in angle brackets: UInt<32> ─────────────────────────────
syn match   archAngleNum   /<[0-9][0-9_]*>/  contained
syn region  archTypeParam  start=/</ end=/>/ contains=archAngleNum,archType,@archIdents oneline

" ── Identifiers by naming convention ─────────────────────────────────────────
" UPPER_SNAKE → parameters / constants
syn match   archParam      /\<[A-Z][A-Z0-9_]\{2,}\>/
" PascalCase → types, module names, enum names (starts uppercase, mixed case)
syn match   archTypeName   /\<[A-Z][a-zA-Z0-9]\+\>/
" snake_case → signals, ports, locals (contains a lowercase letter after _)
" (default Identifier covers the rest)

" ── Enum variant: Name::Variant ───────────────────────────────────────────────
syn match   archEnumVariant  /\<[A-Z][a-zA-Z0-9]*::[A-Z][a-zA-Z0-9]*\>/

" ── String / char (not standard in ARCH but included for completeness) ────────
syn region  archString     start=/"/ end=/"/ skip=/\\"/

" ── Comments ─────────────────────────────────────────────────────────────────
syn match   archLineComment  "//.*$"
" Unicode box-drawing inside comments looks fine as-is

" ── Highlight groups ─────────────────────────────────────────────────────────
hi def link archConstruct    Keyword
hi def link archEnd          Keyword
hi def link archBlock        Statement
hi def link archControl      Conditional
hi def link archAttr         Special
hi def link archDir          Type
hi def link archType         Type
hi def link archParamKind    StorageClass
hi def link archBool         Boolean
hi def link archTodo         Todo
hi def link archCast         Operator
hi def link archBuiltinFn    Function
hi def link archBoolOp       Operator
hi def link archOp           Operator
hi def link archSizedLit     Number
hi def link archHexLit       Number
hi def link archBinLit       Number
hi def link archDecLit       Number
hi def link archParam        Constant
hi def link archTypeName     Structure
hi def link archEnumVariant  Constant
hi def link archString       String
hi def link archLineComment  Comment

let b:current_syntax = "arch"
