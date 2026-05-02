# Arch HDL — AI Reference Card

*Compact AI context for hardware generation · v0.60.0 · Put this in context, add design intent, paste compiler errors to self-correct.*

---

## 1. Universal Block Schema

Every construct uses the same layout:

```
keyword Name
  param NAME: const = value;          // untyped int → parameter int (32-bit)
  param NAME[hi:lo]: const = value;   // width-qualified → parameter [hi:lo]
  param NAME: type = SomeType;        // compile-time type parameter

  port name: in TypeExpr;
  port name: out TypeExpr;
  port name: initiator BusName;       // bus port (initiator perspective)
  port name: target BusName;          // bus port (directions flipped)

  generate for i in 0..N-1           // generated ports / instances
    port p[i]: in UInt<8>;
  end generate for i

  generate if PARAM > 0              // conditional ports
    port opt: out Bool;
  end generate if

  assert name: expression;
  cover name: expression;
end keyword Name
```

**Signal assignment:**

```
// ── Combinational (let) ──────────────────────────────────────────────────
let x: UInt<32> = a + b;            // declare new wire, type required
let out_port = acc;                  // assign to existing output port or wire
                                     //   (no type — inferred from declaration)
wire w: UInt<8>;                     // declare wire; drive it in comb block
comb                                 // multi-assignment / conditional
  if sel
    w = a;
  else
    w = b;
  end if
  other_port = w;
end comb

// ── Registered (seq) ─────────────────────────────────────────────────────
reg r: T reset rst => 0 sync high;  // register with reset (value after =>)
reg r: T init 0 reset rst => 0;    // init sets SV declaration initializer
reg r: T reset none;                 // register without reset
reg default: reset rst => 0;         // wildcard default for all regs in scope
reg r: UInt<8>;                      // inherits reset from reg default
reg data: UInt<32> guard valid_r;    // guarded (no reset) — valid_r tells consumers when data is live;
                                     //   --check-uninit silences spurious read warnings AND catches the
                                     //   producer bug (valid_r=true but data never written)
// Registered output port — latency is visible in the port signature.
// Assign with `@N <= Y` in seq; reads as "Y will be in q N cycles from now".
port q: out pipe_reg<UInt<8>, 1> reset rst => 0;    // 1-cycle registered output
port q: out pipe_reg<UInt<8>, 3> reset rst => 0;    // 3-cycle output pipe
                                                     // q@N <= Y in seq; bare q <= Y is an error for N>1

// Legacy `port reg` — DEPRECATED (identical SV as pipe_reg<T, 1>, but
// hides the latency in a keyword instead of the port signature).
// Still accepted; triggers a compile-time warning. Silence with
// ARCH_NO_DEPRECATIONS=1 during migration.
port reg q: out UInt<8> reset rst => 0;              // 1-cycle registered output
port reg dout: out UInt<32> guard dout_valid;        // port form of guarded reg
pipe_reg delayed: source stages 3;                   // N-stage internal delay chain

// OUTPUT TIMING (critical for FSM outputs):
//   port q: out T                   → comb (=), output reflects state same cycle
//   port q: out pipe_reg<T, 1> ...  → seq q@1 <= ...; 1-cycle latency
//   port q: out pipe_reg<T, N> ...  → seq q@N <= ...; N-cycle latency (cascade)
// Use plain port + comb for zero-latency FSM outputs (e.g. cocotb expects same-cycle response).
// Use pipe_reg<T, N> for glitch-free registered outputs; N visible to LLMs in the signature.

default seq on clk rising;           // set default clock for all seq blocks

seq on clk rising                    // clocked process — uses <=
  r <= expr;                         // compiler auto-generates if(rst) guard
  p <= expr;                         // no reset guard (reset none)
end seq

seq r <= expr;                       // one-line seq (uses default clock)
```

**Conditionals:** use `elsif` (one word), NOT `else if`:

```
if cond_a
  r <= val_a;
elsif cond_b
  r <= val_b;
else
  r <= val_c;
end if
```

**unique if / unique match** — assert mutual exclusivity; emits SV `unique if` / `unique case`:

```
unique if sel == 0
  y = a;
else
  y = b;
end if

unique match opcode
  0 => r <= a;
  1 => r <= b;
  _ => r <= 0;
end match
```

`match` in `comb` uses `=`; in `seq` uses `<=`. Exhaustive enum match (all variants listed) satisfies the latch check without needing `_` wildcard:

```
comb
  match color
    Color::Red   => out = 1;
    Color::Green => out = 2;
    Color::Blue  => out = 3;
  end match
end comb
```

**For loops** (runtime, in comb/seq blocks):

```
for i in 0..7
  out[i] = data[7 - i];            // inclusive range, emits SV for loop
end for

for i in {0, 3, 7, 15}
  mask[i] = true;                  // value-list, compile-time unrolled
end for
```

Range `for` = runtime SV loop; value-list `for` = compile-time unroll; `generate for` = ports/instances.

---

## 2. Types

```
UInt<N>  SInt<N>  Bool  Bit
Clock<Domain>  Reset<Sync|Async, High|Low>   // polarity defaults High
Vec<T,N>
struct S  { f: T; }
enum E   { A, B, }
```

- `SysDomain` is built-in — no `domain SysDomain end domain SysDomain` needed
- `Bool` and `UInt<1>` are identical — freely assignable, bitwise ops on 1-bit return Bool
- `Bit` is an alias for `UInt<1>`
- No current `Future<T>` / `await` / user-visible `Token<T>` API. TLM concurrency is expressed with worker threads, `generate_for`, direct-call `fork/join` cohorts, and RHS-fork groups.
- `Clock<Domain>` may be `out` — use for passthrough (`comb clk_out = clk_in;`), gating (`comb clk_out = clk_in & en;`), or division. For integrated latch-based gating use the `clkgate` construct.
- **`struct` packed bit layout: declaration-first = MSB** (SV convention). A TB reading a struct-typed signal as an integer finds the first-declared field in the top bits, last-declared at the LSBs.

**Width conversions** (always explicit):

```
x.trunc<N>()   // lowest N bits (SV: N'(x)); N must be < source width
x.zext<N>()    // zero-extend; N must be > source width
x.sext<N>()    // sign-extend; N must be > source width
x.resize<N>()  // direction-agnostic: widens or narrows; no direction check
               // UInt/Bool → N'($unsigned(x)); SInt → N'($signed(x))
               // use when N is a param or direction varies by instantiation
x[hi:lo]       // bit-slice (SV: x[hi:lo])
x[i]           // single bit extract
```

**Cast direction rules** (compiler-enforced when source width is known):

| Method | Error if N == src | Error if N < src | Error if N > src |
|--------|------------------|-----------------|-----------------|
| `.trunc<N>()` | yes (no-op) | — | yes (widens) |
| `.zext<N>()` | yes (no-op) | yes (narrows) | — |
| `.sext<N>()` | yes (no-op) | yes (narrows) | — |
| `.resize<N>()` | — | — | — |

**Built-in functions:**

```
onehot(index)  // one-hot decode: 1 << index; width inferred from context
$clog2(expr)   // ceiling log2 (SV: $clog2(expr))
signed(expr)   // same-width reinterpret to SInt (SV: $signed(expr))
unsigned(expr) // same-width reinterpret to UInt (SV: $unsigned(expr))
```

**Signedness reinterpret** (same width, no N needed):

```
signed(x)      // UInt<8> → SInt<8>  (SV: $signed(x))
unsigned(x)    // SInt<8> → UInt<8>  (SV: $unsigned(x))
```

Use `signed()` for signed arithmetic chains: `signed(a) + signed(b)` → `SInt<9>`

**Arithmetic:** `UInt<8> + UInt<8>` → `UInt<9>` (auto-widen); must `.trunc<8>()` to assign back.

**Wrapping arithmetic** (no auto-widen; result = `max(W(a),W(b))`):

```
a +% b    // wrapping add   → SV: W'(a + b)
a -% b    // wrapping sub   → SV: W'(a - b)
a *% b    // wrapping mul   → SV: W'(a * b)
```

Prefer wrapping ops over `.trunc<N>()` when the intent is deliberate modular arithmetic:
`let x: UInt<8> = a +% b;`  is equivalent to  `let x: UInt<8> = (a + b).trunc<8>();`

`$clog2(expr)` supported in type args: `UInt<$clog2(DEPTH)>`

**Vec methods** (parallel-reduction; fully unrolled; no runtime iteration):

```
vec.any(pred)        → Bool              // OR-reduce of per-element compares
vec.all(pred)        → Bool              // AND-reduce
vec.count(pred)      → UInt<clog2(N+1)>  // popcount of hits
vec.contains(x)      → Bool              // shorthand for vec.any(item == x)
vec.find_first(pred) → struct {found: Bool, index: UInt<clog2(N)>}
vec.reduce_or()      → T                 // no predicate; elementwise OR
vec.reduce_and()     → T                 // no predicate; elementwise AND
vec.reduce_xor()     → T                 // no predicate; elementwise XOR
```

Predicates reference `item` (element, type T) and `index` (position, UInt<clog2(N)>) — context-sensitive binders, only in scope inside the argument expression. No lambda syntax; follows SystemVerilog's `with (item)` convention.

```arch
// Canonical search — destructure the result:
let {found, index} = haystack.find_first(item == needle);
if found
  result = haystack[index];
end if

// Index-aware predicate:
let {found, index} = vec.find_first(item == needle and index >= start);

// Simple reductions:
let any_set:  Bool = flags.any(item);
let count:    UInt<3> = bits.count(item == 1'b1);
let has_it:   Bool = haystack.contains(needle);
let parity:   Bool = data.reduce_xor();
```

v1 scope; `map`, `fold`, `zip`, `find_last`, `take_while` are deferred.

**Struct destructuring in `let`:**

```arch
let {field1, field2} = struct_expr;   // binds two locals to the struct's named fields
```

- Any struct-typed RHS (user struct, find_first result, module output struct, etc.)
- Partial destructure is fine — listed fields must exist, rest are ignored
- No type annotation; types are inferred from the struct definition
- No rename form in v1 — use `let alias = s.field;` if you need a different local name

**Bit ops:**

```
{a, b}          // concat (MSB first) → SV {a, b}
{N{expr}}       // replication → SV {N{expr}}
{{8{sign}}, b}  // nestable: 16-bit sign-extended value
```

---

## 3. Expressions & Operators

```
Arithmetic:  + - * / %   (auto-widen)
Wrapping:    +% -% *%    (no-widen; result width = max(W(a),W(b)))
Comparison:  == != < > <= >=
Logical:     and or not
Bitwise:     & | ^ ~ << >>
SVA-only:    |->  (overlap implication, same-cycle)
             |=>  (next-cycle implication)
             // both legal only inside `assert`/`cover` bodies; outside SVA,
             // use `(!a) || b` for plain Boolean implication.
             // The legacy `implies` keyword is a deprecated alias for `|->`.
Ternary:     cond ? a : b      // right-associative; chains for priority muxes
Match:       match x { E::A => val1, E::B => val2, _ => default }
Set member:  expr inside {val1, val2, lo..hi}   // returns Bool, emits SV inside
Field:       s.field
Index:       a[i]
Call:        FnName(arg1, arg2)    // overload-resolved by argument types
Enum:        E::Variant
Struct lit:  S { f: val }
Sized lit:   8'hFF  16'd1024  4'b1010   // Verilog-style
todo!        // compilable placeholder; warns at compile, aborts at sim runtime
```

---

## 4. Construct Cards

### module

Combinational or registered logic.

```
module Name
  param W: const = 8;
  local param ADDR: const = $clog2(W);  // localparam: not overridable
  param CFG: Mode = Mode::Fast;          // enum-typed param (type-safe)

  port clk: in Clock<D>;
  port rst: in Reset<Sync>;
  port a:   in UInt<W>;
  port y:   out UInt<W>;
  port v:   in unpacked Vec<UInt<8>, 4>;  // SV unpacked-array port (interop hatch);
                                          // default Vec<T,N> ports emit packed multi-dim.

  reg default: reset rst => 0;           // wildcard default for all regs
  reg r: UInt<W>;                        // inherits reset from default
  port reg y: out UInt<W>;              // output port + register combined
  pipe_reg d: r stages 2;               // 2-stage delay of r (read-only)

  default seq on clk rising;

  seq on clk rising
    r <= a;
    y <= r;
  end seq

  seq r <= a;                            // one-line seq (uses default clk)
end module Name
```

Notes:
- No implicit latches: `comb` signals must be assigned on all paths (missing `else` = error)
- Single driver per signal (error)
- All ports must be connected at instantiation

**Instantiation:**

```
inst u: Name
  param W = 16;
  clk       <- clk;
  a         <- sig;
  y         -> out;
  write.en  <- wr_en;        // port group member: write.en → write_en in SV
  read[0].addr <- sel;       // indexed: read[0].addr → read0_addr
  read[1].data -> out_b;     // index must be integer literal
  axi_rd   -> m_axi_mm2s;   // whole-bus connection (expands per-signal)
end inst u
```

Hierarchical refs **FORBIDDEN**: `inst_name.port` is a compile error. Connect outputs with `-> wire` and use `wire` in enclosing scope.

---

### function

Pure combinational, overloadable.

```
function AddSat(a: UInt<8>, b: UInt<8>) -> UInt<8>
  let sum: UInt<9> = a.zext<9>() + b.zext<9>();
  return sum[8] ? 8'hFF : sum[7:0];
end function AddSat
```

- No state, no clock
- Overloading: same name, different arg types (mangled in SV)
- Emits SV `function automatic`

---

### pipeline

Staged datapath — compiler generates hazard logic.

```
pipeline Name
  port clk: in Clock<D>;
  port rst: in Reset<Sync>;

  stage Fetch
    stall when !in_valid;
    reg r1: T reset rst => 0;
    seq on clk rising
      r1 <= in;
    end seq
  end stage Fetch

  stage Exec
    reg r2: T reset rst => 0;
    seq on clk rising
      r2 <= Fetch.r1;             // cross-stage reference
    end seq
    inst alu0: Alu
      a <- Fetch.r1;
    end inst alu0
  end stage Exec

  flush Fetch when mispredict;        // bubble-only (default)
  flush Fetch when secret_path clear; // also reset stage data regs
end pipeline Name
```

Compiler generates:
- Per-stage `valid_r` registers
- Stall chain (backpressure)
- Flush masks, comb wire declarations

Cross-stage refs rewritten: `Fetch.pc` → `fetch_pc` in SV.

**Cross-stage rule:** in a stage's data-flow code (comb/seq), `<Stage>.<field>`
references are allowed only for self and the immediately preceding stage.
Backward references that skip ≥1 stage emit a direct combinational path
through intermediate registers — rejected at typecheck. Forward reads
(Decode → Execute for hazard checks) and references inside `stall when` /
`flush when` / `forward` clauses are allowed.

`flush <Stage> when <cond>` clears `valid_r` only (bubble). Add the
`clear` modifier to also reset every data register in `<Stage>` to its
declared reset value — useful for security / speculation scenarios where
stale data in flushed regs is a hazard.

`valid_r` per-stage for output gating:
- `valid_r <= start;` in first-stage `seq` overrides default (=1)
- `done = valid_r;` in last-stage `comb` signals pipeline output valid

---

### fsm

Finite state machine — compiler checks exhaustive transitions.

```
fsm Name
  port clk:      in Clock<D>;
  port rst:      in Reset<Sync>;
  port active:   out Bool;
  port fire_irq: out Bool;

  state [Idle, Running, Done]
  default state Idle;

  default
    comb
      active   = false;
      fire_irq = false;
    end comb
  end default

  state Idle
    -> Running when start;
  end state Idle

  state Running
    comb active = true; end comb
    -> Done    when all_done;
    -> Running when not all_done;
  end state Running

  state Done
    comb fire_irq = true; end comb
    -> Idle;                          // unconditional: always advance
  end state Done
end fsm Name
```

- `default state` required (reset value)
- `default ... end default` provides defaults emitted before state `case`; states only override what differs
- Implicit hold: if no transition fires, FSM stays in current state — no `-> Self when true` needed
- Multiple transitions: mutual exclusivity checked; `unique if` emitted when exclusive, `priority if` otherwise

**FSM datapath extension** — `reg`, `let`, and `seq` inside FSM states:

```
fsm MulDiv
  reg acc_r: UInt<64> reset rst => 0;
  let done: Bool = (cycle_r == 31);

  state Idle
    seq on clk rising
      acc_r <= 0;
    end seq
    -> Multiply when req_valid;
  end state Idle
end fsm MulDiv
```

Co-locates control and datapath — emits separate `always_ff` and `always_comb` in SV.

---

### thread

Sequential multi-cycle block — compiler lowers to a synthesizable FSM. Use instead of
`fsm` for straight-line protocol logic with explicit `wait` points.

```
module M
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;

  // Ports driven by threads must declare sharing policy when multiple
  // threads drive the same port.
  port r_ready:   out Bool shared(or);  // OR-merged across threads
  port push_valid: out Bool shared(or);

  resource ar_ch;                       // mutex resource for lock blocks

  thread MyThread on clk rising, rst high
    // Optional soft-reset clause — fires from ANY state, resets to S0.
    // Only seq assigns allowed inside default when.
    default when start and not active_r
      active_r  <= true;
      xfer_ctr  <= 0;
    end default

    // ── Sequential body ─────────────────────────────────────────────
    wait until active_r;                // S0: block until condition true

    do                                  // S1: drive comb outputs while waiting
      ar_valid = 1;
      ar_addr  = addr_r;
    until ar_ready;                     // advance when ar_ready

    xfer_ctr <= xfer_ctr + 1;          // seq assign fires on exit edge of S1

    wait 4 cycle;                       // S2: stall for exactly 4 cycles

    for b in 0..burst_len_r - 1        // S3–S4: loop with runtime bound
      do
        r_ready    = 1;
        push_valid = r_valid;
      until r_valid and push_ready;
    end for

    lock ar_ch                          // S5: acquire mutex; zero-cycle if free
      ar_valid = 1;
      ar_id    = id_r;
    until ar_ready;                     // release on exit
    end lock ar_ch

    fork                                // S6–S7: AW and W channel in parallel
      do aw_valid = 1; until aw_ready;
    and
      do  w_valid = 1; until  w_ready;
    join

  end thread MyThread
end module M
```

**Statement summary:**

| Statement | Meaning | State boundary? |
|-----------|---------|-----------------|
| `x = expr` | Comb assign (drives output while in this state) | No |
| `x <= expr` | Seq assign (fires when state exits) | No |
| `wait until cond` | Block until condition true | Yes — new state |
| `wait N cycle` | Stall exactly N clock cycles | Yes — new state |
| `do … until cond` | Drive comb + seq while condition false | Yes — hold state |
| `for i in s..e { … }` | Loop body; `i` replaced by `_loop_cnt` reg | Yes — per-body |
| `lock res { … } end lock res` | Acquire mutex, execute body, release | Yes — per-body |
| `fork … and … join` | Execute branches in parallel (product-state FSM) | Yes — product |
| `if/else` (no waits) | Same-state conditional comb/seq | No |

**Rules:**
- `thread Name on clk rising, rst high` — clock edge and reset polarity required
- `default when cond … end default` must appear **before** the thread body; only seq assigns inside
- `lock` blocks must **not** be nested — compile error (mutual exclusion guarantee)
- `thread once` — FSM holds in terminal state instead of looping back to S0
- `generate_for i in 0..N-1 / thread T_i … end thread T_i / end generate_for` — N identical threads
- Multiple threads in one module share one `always_ff` — no multi-driver conflicts
- Thread-driven `reg` declarations are **automatically** lifted to the `_ModuleName_threads` submodule

**Lock deadlock freedom:** the fixed-priority arbiter (`grant[i] = req[i] && !grant[j<i]`)
makes the waits-for graph acyclic — thread 0 always wins, thread N waits only for threads
with lower index. See `doc/thread_lowering_algorithm.md` for proof.

---

### fifo

Sync or dual-clock async FIFO (gray-code auto-generated). `kind: fifo` (default) | `lifo`.

```
fifo Name
  kind lifo;                          // optional, default fifo
  param DEPTH: const = 64;
  param T: type = UInt<32>;       // REQUIRED — sets memory element type

  port clk: in Clock<D>;             // or wr_clk + rd_clk for async
  port rst: in Reset<Sync>;
  port push_valid: in Bool;
  port push_ready: out Bool;
  port push_data:  in T;          // must use the type param, NOT UInt<N>
  port pop_valid:  out Bool;
  port pop_ready:  in Bool;
  port pop_data:   out T;         // must use the type param, NOT UInt<N>
end fifo Name
```

- A `type` parameter (e.g. `param T: type = UInt<32>`) is **required** — it sets the internal memory element width. Using `in UInt<32>` directly on push_data/pop_data without a type parameter is a compile error.
- `param OVERFLOW: const = 1;` — optional. When set, `push_ready` is always high and writing to a full FIFO overwrites the oldest entry (circular buffer / drop-oldest mode). Default 0 = block when full.
- Dual-clock: replace `clk` with `wr_clk: in Clock<WrD>` + `rd_clk: in Clock<RdD>`; compiler adds gray-code CDC
- `kind lifo` restricted to single-clock only

---

### synchronizer

CDC synchronizer. `kind: ff | gray | handshake | reset | pulse`.

```
synchronizer Name
  kind ff;                            // ff (default) | gray | handshake | reset | pulse
  param STAGES: const = 2;           // 2 or 3 (default 2)
  port src_clk:  in Clock<SrcDomain>;
  port dst_clk:  in Clock<DstDomain>;
  port rst:      in Reset<Async>;
  port data_in:  in Bool;            // or UInt<N> for multi-bit
  port data_out: out Bool;
end synchronizer Name
```

Strategies:
- `kind ff` — N-stage FF shift chain on dst clock. Best for 1-bit signals.
- `kind gray` — Binary→gray encode, FF chain, gray→binary decode. Safe for multi-bit counters/pointers.
- `kind handshake` — Req/ack toggle protocol. Safe for arbitrary multi-bit data.
- `kind reset` — Async assert (immediate), sync deassert through FF chain. Bool only.
- `kind pulse` — Toggle in src domain, sync FF chain, edge-detect in dst. Bool only.

Notes:
- Two `Clock` ports must reference different domains (compile error otherwise)
- `kind ff` with multi-bit data (`UInt<N>` where N>1) warns — use `kind gray` or `kind handshake`
- `kind reset` and `kind pulse` error if data is not `Bool`
- CDC checking extends across `inst` boundaries; comb→seq crossings across domains are compile errors

---

### ram

FPGA BRAM / ASIC SRAM.

```
ram Name
  kind simple_dual;                   // single | simple_dual | true_dual
  latency 1;                          // 0=async read, 1=sync, 2=sync_out
  param DEPTH: const = 1024;
  port clk: in Clock<D>;

  store
    weights: Vec<SInt<8>,  DEPTH>;   // multiple named logical address ranges
    biases:  Vec<SInt<16>, DEPTH>;
  end store

  port rd
    en:   in Bool;
    addr: in UInt<10>;
    data: out SInt<8>;
  end port rd

  port wr
    en:   in Bool;
    addr: in UInt<10>;
    data: in SInt<8>;
  end port wr

  init: zero;                         // zero | none | file 'x.hex'
end ram Name
```

---

### cam

Content-addressable lookup. Combinational match of a search key against a vector of (valid, key) entries; single write port for set/clear by index. Use for cache MSHR address lookup, per-flow tables, scoreboard tag CAM, or any design that today hand-rolls `Vec<reg>` + a comb match loop. Compose with `linklist` (CAM finds head index → linklist owns the chain) for content-keyed multi-list designs.

```
cam Mshr_Addr_Cam
  param DEPTH: const = 32;            // required
  param KEY_W: const = 10;            // required

  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;

  // Write port: set (write_set=true → insert valid+key) or clear (false → invalidate)
  port write_valid: in Bool;
  port write_idx:   in UInt<5>;       // $clog2(DEPTH)
  port write_key:   in UInt<10>;      // KEY_W
  port write_set:   in Bool;

  // Search port: combinational
  port search_key:   in  UInt<10>;
  port search_mask:  out UInt<32>;    // bitmask of matches; zero if no match
  port search_any:   out Bool;
  port search_first: out UInt<5>;     // LSB-priority first match (gate with search_any)
end cam Mshr_Addr_Cam
```

**Semantics:**
- One write per cycle. Search reads pre-write state on the same cycle as a write; the write commits on the next clock edge.
- `search_first` is the LSB-priority first set bit of `search_mask`; consumers should qualify with `search_any` (it reads as 0 when there is no match).
- v1 is exact-match only (no TCAM/wildcards), no value payload (pair with a `ram` indexed by `search_first` to recover an associated value), no built-in replacement policy (the user picks the index to write — typically a free-slot priority encoder over `~entry_valid_r`).

**Dual-write port (v2):** for designs with two concurrent state-update streams (e.g., MSHR with simultaneous allocate + finalize), add the optional `write2_*` port set:

```
  port write2_valid: in Bool;
  port write2_idx:   in UInt<5>;
  port write2_key:   in UInt<10>;
  port write2_set:   in Bool;
```

All four are required together (all-or-nothing). On the same edge: different indices both commit; same index → port 2 wins (last-write). Map your "winner" stream to port 2.

**Value payload (v3):** absorb the per-entry value (otherwise kept in a parallel `Vec<UInt<W>, DEPTH>`) into the cam itself by adding `param VAL_W` + the matching write/read ports:

```
  param VAL_W: const = 32;
  port write_value: in UInt<32>;
  port read_value:  out UInt<32>;
  // and write2_value if dual-write is enabled
```

All-or-nothing within the value bundle. `read_value = entry_value_r[search_first]`; gate with `search_any` (reads as 0 when no entry matches). Use this for TLB virtual→physical, MAC table MAC→port, scoreboard tag→state. Don't use it when the design re-priority-encodes against an external mask (different index than `search_first`) — keep a separate `Vec` for that case.

---

### counter

Configurable counter. `kind: wrap | saturate | gray | one_hot | johnson`.

```
counter Name
  param T: const = 8;
  port clk:   in Clock<D>;
  port rst:   in Reset<Sync>;
  port en:    in Bool;
  port count: out UInt<T>;
  port at_max: out Bool;
  port at_min: out Bool;
  kind wrap;
  direction: up;                      // up | down | up_down
end counter Name
```

---

### arbiter

N-requester, policy-driven grant with optional hook and latency.

```
arbiter Name
  param N: const = 4;
  port clk: in Clock<D>;
  port rst: in Reset<Sync>;
  ports[N] req
    valid: in Bool;
    ready: out Bool;
  end ports req
  port grant_valid:      out Bool;
  port grant_requester:  out UInt<$clog2(N)>;
  policy: round_robin;               // round_robin | priority | lru | weighted<W> | <FnName>
  latency 1;                         // 1=comb grant (default), 2=+1 stage, 3=+2 stages
end arbiter Name
```

**Custom policy:** define a `function` in the same file and reference by name. The function receives `req_mask` (one-hot pending) and `last_grant` (one-hot previous winner) and returns a one-hot grant mask.

```
function MyGrantFn(req_mask: UInt<4>, last_grant: UInt<4>, extra_port: UInt<8>) -> UInt<4>
  let masked: UInt<4> = req_mask & (last_grant ^ 0xF);
  let pick: UInt<4>   = masked != 0 ? masked : req_mask;
  let pick_neg: UInt<5> = (pick ^ 0xF).zext<5>() + 1;
  return pick & pick_neg.trunc<4>();
end function MyGrantFn

arbiter CustomArb
  policy MyGrantFn;
  param N: const = 4;
  port clk:        in Clock<D>;
  port rst:        in Reset<Sync>;
  port extra_port: in UInt<8>;      // extra port passed through to hook
  ports[N] req
    valid: in Bool;
    ready: out Bool;
  end ports req
  port grant_valid:     out Bool;
  port grant_requester: out UInt<2>;
  hook grant_select(req_mask: UInt<4>, last_grant: UInt<4>, extra_port: UInt<8>) -> UInt<4>
    = MyGrantFn(req_mask, last_grant, extra_port);
end arbiter CustomArb
```

Hook args bind to: hook params (internal signals) or user-declared ports/params on the arbiter.

---

### regfile

Multi-port register file.

```
regfile Name
  param DEPTH: const = 32;
  param T: const = 32;
  port clk: in Clock<D>;
  port rst: in Reset<Sync>;

  port rd0
    addr: in UInt<5>;
    data: out UInt<T>;
  end port rd0

  port wr0
    en:   in Bool;
    addr: in UInt<5>;
    data: in UInt<T>;
  end port wr0

  kind flop;                         // flop (default) | latch (ASIC area/power)
  flops: external;                   // latch only: external (default, caller flops) | internal (Ibex-style, +1 cycle latency)
  forward write_before_read: false;  // true = enable bypass forwarding
  init [0] = 0;                      // per-index reset values
end regfile Name
```

`kind latch` emits one `always_latch` per row with one-hot enable decoding. Default `flops: external` requires the caller to drive `addr`/`data` from a flop (typecheck enforces). `flops: internal` makes the regfile emit its own `we_q`/`waddr_q`/`wdata_q` sample flops + ICG-equivalent gating; caller may drive write pins combinationally, write lands one cycle later. Latch RFs require `@(negedge clk)`-driven testbenches (Ibex convention).

---

### linklist

Singly/doubly/circular linked list with built-in free-list and FSM controllers.

```
linklist Name
  param DEPTH: const = 256;
  param DATA: type = UInt<32>;
  port clk: in Clock<D>;
  port rst: in Reset<Sync>;
  kind singly;                        // singly | doubly | circular_singly | circular_doubly
  track tail:   true;
  track length: true;

  op insert_tail
    latency: 2;
    port req_valid:   in Bool;
    port req_ready:   out Bool;
    port req_data:    in DATA;
    port resp_valid:  out Bool;
    port resp_handle: out UInt<3>;
  end op insert_tail

  op delete_head
    latency: 2;
    port req_valid:  in Bool;
    port req_ready:  out Bool;
    port resp_valid: out Bool;
    port resp_data:  out DATA;
  end op delete_head

  port empty:  out Bool;
  port full:   out Bool;
  port length: out UInt<4>;
end linklist Name
```

Operations (via `op` port): `insert_head`, `insert_tail`, `insert_after`, `delete_head`, `delete`, `next`, `prev` (doubly), `alloc`, `free`, `read_data`, `write_data`. 2-cycle latency per operation.

**Multi-head (NUM_HEADS > 1)**: add `param NUM_HEADS: const = N;` to turn the linklist into K homogeneous chains sharing one node pool + free list. Each head-addressed op (`insert_*`, `delete_*`) must then declare `port req_head_idx: in UInt<$clog2(NUM_HEADS)>`. Typical use: MSHR, per-flow queues, per-address pending tables. `NUM_HEADS == 1` (default) emits byte-identical SV to before. Today `insert_tail` + `delete_head` are fully supported in multi-head; other head-addressed ops stage in a follow-up. For *heterogeneous* named lists with different depths/types sharing a pool, use `linklist_pool` (spec §12.6).

---

### generate

Compile-time port and instance generation.

```
generate for i in 0..SIZE-1
  port a[i]: in SInt<8>;
  inst pe[i]: ProcElem
    clk   <- clk;
    a_in  <- a[i];
    sum_in <- i == 0 ? 0 : pe[i-1].sum_out;  // boundary expression
  end inst pe[i]
end generate for i

generate if DEBUG_EN
  port dbg: out UInt<32>;
end generate if
```

- Generates real named ports: caller uses `a[0]`, `a[3]`, etc.
- Type-checked per index
- `generate if`: port does not exist when condition is false — accessing it is a compile error

---

### bus

Reusable port bundle with initiator/target perspectives.

```
bus AxiLite
  param ADDR_W: const = 32;
  aw_valid: out Bool;                 // direction from initiator's perspective
  aw_ready: in  Bool;
  aw_addr:  out UInt<ADDR_W>;
end bus AxiLite
```

Usage:

```
module Master
  port axi: initiator AxiLite;       // directions as declared
  comb
    axi.aw_valid = 1;                // dot notation for signal access
    axi.aw_addr  = addr_r;
  end comb
end module Master

module Slave
  port axi: target AxiLite;         // directions FLIPPED (in↔out)
end module Slave
```

SV output: flattened to individual ports (`axi_aw_valid`, `axi_aw_addr`, etc.).

### handshake_channel (inside bus)

Valid/ready/payload channels as a compile-time sum type. Six variants; compiler derives every individual wire direction from the role keyword.

> **Rename note (v0.44.0):** `handshake_channel` is the current keyword. Legacy `handshake` still parses and emits a deprecation warning; removal scheduled for v0.45.0. Silence with `ARCH_NO_DEPRECATIONS=1`.

```
bus BusAxiLite
  handshake_channel aw: send kind: valid_ready     // this side drives valid + payload, reads ready
    addr: UInt<32>;
    prot: UInt<3>;
  end handshake_channel aw

  handshake_channel b: receive kind: valid_ready   // flipped: reads valid + payload, drives ready
    resp: UInt<2>;
  end handshake_channel b
end bus BusAxiLite
```

**Role keywords**: `send` / `receive` (NOT `in`/`out`). Names payload role; all individual wire directions derived. `target` bus perspective flips everything — a `send` becomes a `receive` on the target.

**Variants**:
- `valid_ready` — full backpressure; AMBA AXI / general streaming
- `valid_only` — fire-and-forget; strobes, interrupts
- `ready_only` — pull model; rare
- `valid_stall` — inverted backpressure; pipeline interlocks
- `req_ack_4phase` — async return-to-zero handshake
- `req_ack_2phase` — async NRZ handshake (Tier-2 SVA deferred)

**Free correctness layers**:
- **Tier 2 SVA**: `arch build` auto-emits protocol assertions (`_auto_hs_<port>_<ch>_<rule>`) for `valid_ready` / `valid_stall` / `req_ack_4phase`. Consumed by Verilator `--assert` and EBMC.
- **Tier 1.5 producer bug**: `arch sim --inputs-start-uninit` warns only when valid is asserted and payload was never `set_`'d.
- **Tier 1.5 consumer bug**: `arch check` warns when a payload is read outside `if <port>.<valid>` (or AND-conjunct of that).

Full spec: `doc/ARCH_HDL_Specification.md` §18a.

### credit_channel (inside bus)

Stateful credit-based flow control. The compiler owns the sender counter, the receiver FIFO, and the Tier-2 protocol SVA. Nests inside a `bus`; no standalone form.

```
bus DmaCh
  credit_channel data: send
    param T:                   type  = UInt<64>;
    param DEPTH:               const = 8;
    param CAN_SEND_REGISTERED: const = 0;   // 1 = flop can_send off counter_next
  end credit_channel data
end bus DmaCh
```

**API** (read-side methods + write-side sugar; **all access is dotted** — `port.<ch>.<wire>`, not underscored):
- Sender: `port.ch.can_send` (read); `port.ch.send(x);` (drives `send_valid=1, send_data=x`); `port.ch.no_send();` (defaults `send_valid=0, send_data=0`).
- Receiver: `port.ch.valid`, `port.ch.data` (read); `port.ch.pop();` (drives `credit_return=1`); `port.ch.no_pop();` (defaults `credit_return=0`).
- The underscored form `port.ch_send_valid` is **rejected** by the compiler — use `port.ch.send_valid` for direct conditional drives, or `no_send()`/`no_pop()` for defaults.

**Canonical pattern**:
```
// Sender
comb
  out.flits.no_send();
  if out.flits.can_send
    out.flits.send(seq_no);
  end if
end comb

// Receiver
comb
  incoming.flits.no_pop();
  if incoming.flits.valid
    incoming.flits.pop();
  end if
end comb
```

**Auto-emitted SVA (Tier 2)**: `_auto_cc_<port>_<ch>_{credit_bounds,send_requires_credit,credit_return_requires_buffered}` under `translate_off/on`.

`arch sim --pybind --test` mirrors both the sender counter and the receiver FIFO in C++ (module construct). Pipeline / thread / arbiter with credit_channel ports are not yet wired.

Full spec: `doc/ARCH_HDL_Specification.md` §18c.

### tlm_method (inside bus)

Transaction-level method sub-construct. Initiator modules *call*; target modules *implement* via a dotted-name thread. Current compiler support is `blocking` plus a small tagged out-of-order slice.

```
bus Mem
  tlm_method read(addr: UInt<32>) -> UInt<64>: blocking;
  tlm_method read_ooo(addr: UInt<32>) -> UInt<64>: out_of_order tags 2;
  tlm_method write(addr: UInt<32>, data: UInt<64>) -> Bool: blocking;
  tlm_method poke(addr: UInt<32>): blocking;       // void
end bus Mem
```

**Target side** — thread name is `port.method(args)`:

```
module MemTarget
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port s:   target Mem;
  port ready: in Bool;
  thread s.read(addr) on clk rising, rst high
    wait until ready;
    return 64'h42;
  end thread s.read
end module MemTarget
```

**Initiator side** — call as RHS of `<=` inside a thread:

```
module Initiator
  port m: initiator Mem;
  reg   d: UInt<64> reset rst => 0;
  thread driver on clk rising, rst high
    d <= m.read(32'h1000);
  end thread driver
end module Initiator
```

Both sides lower to a parent-module state machine (state reg + RegBlock + CombBlock). `arch sim`, `arch sim --pybind --test`, and `arch sim --thread-sim parallel` work through generated C++ models; parallel mode uses the regular sim model for modules whose TLM threads were lowered away.

TLM calls are not general expressions. They are legal only in `thread` bodies as `dst <= port.method(args);` or `dst <= fork port.method(args);`; `comb`, `seq`, module-level `let`, module-local `function`, `pipeline`, and `fsm` contexts reject them.

Do not put TLM calls inside runtime `for` loops. Use `generate_for` worker threads for compile-time replication.

**Concurrent initiator cohorts** — multiple direct worker calls on one method lower to an arbiter plus response router:

```
module LoadPair
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync>;
  port m:   initiator Mem;
  reg d0: UInt<64> reset rst => 0;
  reg d1: UInt<64> reset rst => 0;

  thread workers on clk rising, rst high
    fork
      d0 <= m.read(32'h10);
    and
      d1 <= m.read(32'h20);
    join
  end thread workers
end module LoadPair
```

Supported cohort shapes: multiple direct named worker threads, `generate_for` worker threads, and one direct-call `fork ... and ... join` thread. Blocking cohorts route responses by issue-order FIFO. `out_of_order tags N` cohorts drive `<method>_req_tag` and route by `<method>_rsp_tag`; target threads latch and echo the tag.

Timed multiple-outstanding issue in one thread:

```
thread driver on clk rising, rst high
  d0 <= fork m.read(32'h10);
  wait 1 cycle;
  d1 <= fork m.read(32'h20);
  join all;
end thread driver
```

`dst <= fork m.read(...);` is a nonblocking TLM issue; `join all;` waits for every forked issue in the group. v1 allows direct forked TLM assignments plus literal `wait N cycle;` offsets, with `join all;` final.

Current restrictions: thread-body call sites only; direct RHS call only (`dst <= m.method(args);` or `dst <= fork m.method(args);`); no TLM calls inside runtime `for` loops; one call per worker/branch/forked issue; same clock/reset per cohort; literal tag count only; RHS-fork offsets require literal `wait N cycle;`; no nested/composed TLM calls; no `pipelined`; no `burst`; no `Future<T>`/`await`.

Full spec: `doc/ARCH_HDL_Specification.md` §18d. Design + v2 roadmap: `doc/plan_tlm_method.md`.

### Standard bus library (zero-setup `use`)

The ARCH compiler ships curated bus definitions under `<install>/stdlib/`. Use any of them from any file with no path setup:

```
use BusAxiStream;
port m_axis: initiator BusAxiStream<DATA_W=32, USE_LAST=1, USE_KEEP=1>;

use BusAxiLite;
port s_axi:  target BusAxiLite<ADDR_W=12, DATA_W=32>;

use BusApb;
port s_apb:  target BusApb<ADDR_W=16, DATA_W=32, USE_PPROT=1, USE_PSTRB=1>;
```

**v1 stdlib buses**:
- `BusAxiStream` — AXI4-Stream. Params: `DATA_W`, `USE_LAST`, `USE_KEEP`, `USE_STRB`, `ID_W`, `DEST_W`, `USER_W`.
- `BusAxiLite` — AXI4-Lite memory-mapped. Params: `ADDR_W`, `DATA_W`. Full 5-channel aw/w/b/ar/r bundle.
- `BusApb` — APB3 / APB4. Params: `ADDR_W`, `DATA_W`, `USE_PPROT`, `USE_PSTRB`.

**Discovery order**: same-dir → `$ARCH_LIB_PATH` → `<install>/stdlib/` (disable with `ARCH_NO_STDLIB=1`; override the install path with `$ARCH_STDLIB_PATH`).

Third-party packages ship the same way — drop `BusMyProto.arch` into `$ARCH_LIB_PATH` and `use BusMyProto;` works identically to the stdlib.

Full spec: `doc/ARCH_HDL_Specification.md` §18b.

---

### template

User-defined interface contract. Compile-time only — no SV emitted.

```
template MyInterface
  param NUM_REQ: const;
  port clk:         in Clock<D>;
  port rst:         in Reset<Sync>;
  port grant_valid: out Bool;
  hook grant_select(req_mask: UInt<4>) -> UInt<4>;  // signature only
end template MyInterface
```

Modules opt in with `implements`:

```
module Foo implements MyInterface
  ...
  hook grant_select(req_mask: UInt<4>) -> UInt<4>
    = FixedGrant(req_mask);          // binding required in implementing module
end module Foo
```

Missing any required param, port, or hook is a compile error.

---

### package

Reusable type/function namespace.

```
// BusPkg.arch  (filename must match package name)
package BusPkg
  domain FastClk
    freq_mhz: 500
  end domain FastClk

  enum BusOp
    Read, Write, Idle
  end enum BusOp

  struct BusReq
    op:   BusOp;
    addr: UInt<32>;
    data: UInt<32>;
  end struct BusReq

  function max(a: UInt<32>, b: UInt<32>) -> UInt<32>
    return a > b ? a : b;
  end function max
end package BusPkg
```

Consumer:

```
use BusPkg;

module Consumer
  port req:      in BusReq;
  port addr_out: out UInt<32>;
  comb addr_out = req.addr;
end module Consumer
```

SV output: `package BusPkg; ... endpackage` + `import BusPkg::*;`

- Contains: `enum`, `struct`, `function`, `param`, `domain` — no modules/pipelines/FSMs
- Domains in a package are shared across files via `use`
- Resolved from same directory or multi-file command line

**Module-local functions** — functions can also be declared inside a module body:

```
module MyModule
  port a: in UInt<8>;
  port b: in UInt<8>;
  port sum: out UInt<8>;

  function add_wrap(x: UInt<8>, y: UInt<8>) -> UInt<8>
    return (x + y).trunc<8>();
  end function add_wrap

  let sum = add_wrap(a, b);
end module MyModule
```

SV output: `function automatic` inside the module block. Use for one-off helpers that don't warrant a package.

Function bodies support: `let`, `return`, `if/elsif/else`, `for` loops, assignment (`=`).

**No-latch rule:** every code path must reach a `return`. An `if` without `else` containing `return` is a compile error. Fix with `else` branch or final `return` after the `if`.

---

### Separate compilation (.archi interface files)

`arch build` emits `.archi` interface files alongside `.sv` by default. These contain only the module signature (params + ports, no body).

```bash
arch build SubModule.arch         # → SubModule.sv + SubModule.archi
arch check TopModule.arch         # auto-discovers SubModule.archi for type-checking
arch build *.arch                 # builds all, respects deps via .archi discovery
```

When `inst sub: SubModule` references an undefined module, the compiler searches for `SubModule.archi` or `SubModule.arch` in the input directory and `ARCH_LIB_PATH`.

---

## 5. Logging

```
log(Level, "TAG", "format %0d", arg);
```

Levels: `Always`, `Low`, `Medium`, `High`, `Full`, `Debug`

- Works in `seq` and `comb` blocks
- Runtime control: `+arch_verbosity=N` (0=Always only … 5=Debug)
- NBA semantics in `seq`: value printed is last cycle's registered value

---

## 6. TLM Method Modes

| Mode | Return type | Use case |
|------|-------------|----------|
| `blocking` | `ret: T` directly | Caller waits for one response. Multiple workers can still be in flight via thread cohorts; responses route by issue-order FIFO. |
| `out_of_order tags N` | `ret: T` directly + hidden tag wires | Multiple direct workers can complete out of order; compiler assigns worker tags and routes responses by `<method>_rsp_tag`. |

```
// Do this:
d <= m.read(addr);             // direct blocking-style call inside a thread
d0 <= fork m.read(addr0);      // nonblocking issue inside a thread
join all;                      // explicit barrier for RHS-fork issues

// Not this:
let f = m.read(addr);           // no Future<T>
await f;                        // no await
```

`pipelined` and `burst` are not current TLM modes. Use worker threads, `generate_for` workers, direct-call `fork ... and ... join`, or RHS-fork groups for multiple outstanding requests.

---

## 7. Simulation & Build

```
arch check F.arch                              // type-check only
arch build F.arch                              // emit SystemVerilog
arch build F.arch -o out.sv                   // combined output
arch build a.arch b.arch                       // multi-file: one .sv per input
arch sim F.arch --tb F_tb.cpp                 // compile + run C++ testbench
arch sim F.arch --tb F_tb.cpp --outdir build/
arch sim F.arch --tb F_tb.cpp --check-uninit  // warn on uninitialized reset-none regs
arch sim F.arch --tb F_tb.cpp --inputs-start-uninit  // warn on reads of TB-undriven inputs (TB calls dut.set_<port>() to mark init)
arch sim F.arch --tb F_tb.cpp --check-uninit-ram  // warn on reads of RAM cells never written (per-cell valid bitmap; init: cells pre-marked; ROMs exempt)
// Runtime bounds checks (always on, no flag): Vec<T,N> indexing, bit-select val[i], part-select val[start+:W]/[-:W]
//   arch sim: out-of-range → ARCH-ERROR to stderr + abort()
//   arch build: also auto-emits concurrent SVA `_auto_bound_<kind>_<n>: assert property` (seq/latch contexts only;
//               comb/let deferred); wrapped in synopsys translate_off/on — free for Verilator/iverilog/formal
// Runtime divide-by-zero (`/` / `%`, non-const divisor only; const divisors elided):
//   arch check: compile error if a param/const let folds to `A / 0` / `A % 0`
//   arch sim: ARCH-ERROR + abort() via _ARCH_DCHK
//   arch build: auto-emits `_auto_div0_<op>_<n>: assert property ((divisor) != 0)` in seq/latch
// Thread spec-contract SVA (off by default; `arch build`/`sim`/`formal` accept `--auto-thread-asserts`):
//   wait until <cond>:  _auto_thread_t{i}_wait_until_s{si}: (rst_inactive && state==si && cond) |=> state==next
//   wait N cycle:       _auto_thread_t{i}_wait_stay_s{si} (cnt!=0 ⇒ stay) + _..._wait_done_s{si} (cnt==0 ⇒ advance)
//   fork/join branches: _auto_thread_t{i}_branch_s{si}_b{bi}: per-(cond,target) implication
//   wrapped in synopsys translate_off/on; reset polarity (Low/High) inverted to the right not-in-reset guard
arch sim F.arch --tb F_tb.cpp --cdc-random    // randomize synchronizer latency
arch sim --pybind --test test_F.py F.arch     // run Python cocotb-style TB through pybind11
                                               //   see doc/arch_sim_cocotb.md for API + portability deltas
arch sim F.arch                                // generate models only (no testbench)
arch formal F.arch                             // direct SMT-LIB2 bounded model checking (z3 by default)
arch formal F.arch --bound 64 --solver bitwuzla
arch formal F.arch --emit-smt model.smt2       // dump the SMT-LIB2 for inspection
arch formal multi.arch --top MyTop             // pick a top module when the file has >1
// v1 scope: flat module, scalar types (UInt/SInt/Bool/Bit), single clock, no sub-`inst`
// Exit codes: 0 all PROVED/HIT · 1 any REFUTED/NOT-REACHED · 2 any INCONCLUSIVE · 3 compile error
```

**arch sim C++ testbench interface** (Verilator-compatible):

```cpp
// Ports map to public fields: dut->clk, dut->rst, dut->data_in
// Wide ports (UInt<N> where N>64) use VlWide<WORDS>:
//   dut->key.data() returns uint32_t* (word 0 = LSB)

// Standard cycle loop:
auto tick = [&]() { dut->clk=0; dut->eval(); dut->clk=1; dut->eval(); };

// Multi-clock: compiler auto-generates tick() from domain freq_mhz
dut.tick();   // auto-toggles fast_clk (200MHz) and slow_clk (50MHz) at 4:1 ratio

// Compile:
// g++ -std=c++17 build/verilated.cpp build/V*.cpp tb.cpp -Ibuild -o sim
```

---

## 8. AI Prompting Patterns

1. **CONSTRUCT-FIRST** (most reliable):
   > "Generate an Arch fifo named InstrQueue, depth 64, element type InstrPacket, single clock SysDomain. Add cover push_when_full."

2. **todo! SCAFFOLDING**:
   > "Generate the skeleton for a 5-stage RISC-V pipeline. Use todo! for all stage bodies."

3. **PASTE COMPILER ERRORS**:
   > "Fix this Arch error: [paste arch check output]"
   Errors are self-sufficient — no spec lookup needed.

4. **ONE CONSTRUCT PER PROMPT**:
   structs → functions → primitives → pipeline → top module → testbench.
   Compile and verify each before moving to the next.

5. **TLM PROGRESSION**:
   Start with a single `tlm_method ... : blocking;` call and target thread. Add worker-thread, `generate_for`, or `fork/join` cohorts only after the single-call handshake works. Use `out_of_order tags N` only when the target can echo tags.

---

## 9. ARCH MCP Server

Tools available when running under the ARCH MCP server:

- `get_construct_syntax(construct)` — returns syntax template + reserved keywords for any construct
- `write_and_check(path, content)` — write `.arch` file + type-check in one call
- `arch_build_and_lint(files, top_module)` — build SV + Verilator lint in one call

Recommended AI workflow: fetch syntax first → `write_and_check` → `arch_build_and_lint`

---

*Arch AI Reference Card · March 2026 · v0.24.0 · `arch check` is your first line of defence*
