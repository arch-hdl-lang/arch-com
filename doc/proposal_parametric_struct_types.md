# Parametric `struct` Types

**Date:** 2026-06-30
**Status:** Proposal — no implementation started
**Scope:** Language + type checker + SV codegen + sim codegen

---

## Problem

ARCH structs are currently monomorphic: every field must have a concrete
width at declaration time. This forces one of three bad workarounds when the
same struct shape is needed at multiple widths:

1. **Duplicate the struct** — `CacheEntry32`, `CacheEntry64`, …  
   Results in divergence and maintenance burden.
2. **Use a wide `UInt<MAX_W>` field and mask manually**  
   Loses the type safety and readability that `struct` is supposed to provide.
3. **Parameterize at the consuming module** and thread `param DATA_W: const`
   into every struct field reference inside that module's body  
   Works, but prevents reuse across module boundaries — the struct can't be
   shared in a `package` and consumed as a typed value.

LLM-generated code suffers from all three: LLMs tend to duplicate structs
rather than parameterize, producing inconsistent field sets across copies.

---

## Motivating Examples

### Cache line entry

```arch
// Today: three separate declarations
struct CacheEntry32
  valid: Bool
  dirty: Bool
  tag:   UInt<20>
  data:  UInt<32>
end struct CacheEntry32

struct CacheEntry64
  valid: Bool
  dirty: Bool
  tag:   UInt<20>
  data:  UInt<64>
end struct CacheEntry64

// With parametric structs: one declaration
struct CacheEntry<DATA_W: const>
  valid: Bool
  dirty: Bool
  tag:   UInt<20>
  data:  UInt<DATA_W>
end struct CacheEntry
```

### ROB entry with configurable operand width

```arch
struct RobEntry<XLEN: const>
  done:     Bool
  rd_addr:  UInt<5>
  result:   UInt<XLEN>
  exc_code: UInt<4>
end struct RobEntry

module ReorderBuffer
  param XLEN:  const = 64;
  param DEPTH: const = 32;

  reg entries: Vec<RobEntry<XLEN>, DEPTH> reset rst => ...;
  // type checker resolves RobEntry<64> at elaboration time
end module ReorderBuffer
```

### AXI beat with configurable data width

```arch
package Axi4

struct AxiWBeat<DATA_W: const>
  data:  UInt<DATA_W>
  strb:  UInt<$clog2(DATA_W)>
  last:  Bool
end struct AxiWBeat

end package Axi4
```

---

## Proposed Syntax

Extend the `struct` grammar with an optional `param` list, using the same
param syntax as all other ARCH constructs:

```
struct_decl = "struct" ident [ "<" param_list ">" ]
                { struct_field }
              "end" "struct" ident ;
```

Where `param_list` is one or more `PARAM: const [= default]` entries,
comma-separated. (Only `const` params in v1; `type` params are deferred.)

Instantiation at a use site:

```arch
port data: out CacheEntry<32>;      // concrete
reg  e:    RobEntry<XLEN>;          // references enclosing module's param
let  beat: AxiWBeat<DATA_W> = ...;  // ditto
```

Instantiation inside `package` `use`:

```arch
use Axi4;
port wdata: out Axi4::AxiWBeat<64>;
```

---

## Type Checker Changes

1. `StructDecl` gains a `params: Vec<ParamDecl>` field (mirrors
   `ConstructCommon::params`).
2. At a use site `CacheEntry<32>`, the checker builds a substitution map
   `{DATA_W → 32}` and applies it to each field's `type_expr` before
   resolving widths. This reuses the existing `subst_ty_params` logic
   already implemented for module param substitution.
3. Two structs with different param values are distinct types — they do not
   unify. `CacheEntry<32>` and `CacheEntry<64>` are different types even
   though they share a declaration.
4. Struct literals at parametric types (`CacheEntry<32> { valid: true, … }`)
   resolve field types after substitution — same as today, just with the
   extra substitution step.
5. Concrete structs (no params) are unaffected — zero behavior change.

---

## SV Codegen Strategy

SV has no parametric `typedef struct packed`. The canonical workaround is
name-mangling: emit one `typedef struct packed` per concrete instantiation.

Mangling rule: `StructName_P1_P2_…` where each param value is rendered as
a decimal integer. Examples:

| ARCH type         | SV typedef name    |
|-------------------|--------------------|
| `CacheEntry<32>`  | `CacheEntry_32`    |
| `CacheEntry<64>`  | `CacheEntry_64`    |
| `RobEntry<32>`    | `RobEntry_32`      |
| `RobEntry<64>`    | `RobEntry_64`      |
| `AxiWBeat<128>`   | `AxiWBeat_128`     |

The codegen pass collects all concrete struct instantiations encountered
during the module walk, emits each typedef once (deduplicated by
`(struct_name, param_values)` key) at the top of the SV output file before
the first module, then uses the mangled name everywhere a type expression
appears in port/reg/wire declarations.

For structs defined inside a `package`, the mangled name is scoped to the
SV package: `typedef struct packed { … } CacheEntry_32;` inside
`package Axi4; endpackage`.

---

## Sim (C++) Codegen Strategy

The native C++ sim already emits `struct` definitions in the generated
header. The same monomorphization applies: one `struct CacheEntry_32 { … }`
per concrete instantiation, with field widths resolved. The struct is emitted
in the header before the module class that first uses it.

---

## Implementation Phases

**Phase 1 (v1 — `const` params, single-file)**

- Parser: accept `struct Name<PARAM: const>` syntax.
- AST: add `params` to `StructDecl`.
- Type checker: resolve field types after substituting concrete param values.
- SV codegen: monomorphize → mangled typedef per concrete instantiation.
- Sim codegen: same monomorphization.
- Scope: single-file use; `package`-scoped parametric structs deferred.
- No default param values in v1.

**Phase 2 — Package support + default params**

- `package` bodies can contain parametric structs.
- `use PkgName;` imports the struct template; `PkgName::Foo<32>` at use sites.
- Default param values: `struct Foo<N: const = 32>` allows `Foo<>` at use.

**Phase 3 — `type` params (deferred, high complexity)**

- `struct Queue<T: type, N: const>` where `T` is any ARCH type.
- Requires full generics / type substitution, much deeper type-checker change.
- Depends on "Function type-parametric overloads" (COMPILER_STATUS item 5)
  landing first, since it solves the same substitution problem.

---

## Out of Scope for v1

- Recursive struct params (a struct whose field type is itself parametric
  with a param derived from the outer struct's param).
- Struct param inheritance or variance.
- `type` param support — that requires full generics.
- Default param values (deferred to Phase 2).

---

## Why Now

- The pattern appears repeatedly in the E203/AXI benchmark suite — multiple
  structs are manually duplicated where a single parametric struct would suffice.
- LLM code generation reliably produces duplicate structs without prompting;
  the language currently offers no better option.
- The implementation is bounded: it's a monomorphization pass that reuses
  the existing `subst_ty_params` machinery and name-mangling convention
  already established for module params and `Vec<T, N>` instantiation.
- No runtime or SV semantics change: it's purely a front-end + codegen
  concern; the emitted SV is valid today (just hand-written where a
  parametric struct would generate it automatically).

---

## Prior Art

- SystemVerilog: no parametric `typedef struct packed`; workarounds are
  macros or parameter-width fields, both untyped and error-prone.
- Chisel/FIRRTL: `Bundle` with runtime width parameters.
- SpinalHDL: `Bundle` parameterized via constructor arguments.
- Bluespec: fully parametric `struct` and `typedef`.

ARCH's approach (compile-time monomorphization with name-mangling) is the
same as Rust/C++ templates lowered to SV — familiar to compiler implementers
and produces readable, tool-portable SV output.
