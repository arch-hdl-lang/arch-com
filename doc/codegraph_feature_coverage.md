# Developer Graph Coverage

The developer graph maps language feature families to reviewed compiler source
locations. It is separate from the user graph of ARCH declarations and instance
relationships. Rebuild it with `arch graph dev-index --root . --out .archdevgraph`.

The bundled map contains 68 feature families covering 147 variants in the ten
core AST enums below, plus named nested constructs such as ports and hooks.

## Coverage Contract

`doc/codegraph_feature_map.json` assigns every variant of these core AST enums
to one feature family through its `covers` metadata:

- `Item`: top-level language declarations and imports.
- `ModuleBodyItem` and `GenItem`: module and generated-body constructs.
- `Stmt`, `ThreadStmt`, and `FunctionBodyItem`: statement and control-flow forms.
- `TypeExpr`: type representations, including floating-point and scaled types.
- `ExprKind`, `BinOp`, and `UnaryOp`: expressions and operators.

The regression independently parses those named enums from `src/ast.rs` with
the Rust parser and compares exact qualified variant sets. Missing enums,
new/renamed/removed variants, and omitted/duplicate mapping entries fail the
check. The required set is not derived from the map itself. Additional feature
families can be mapped without owning a variant in this inventory.

This is a finite, testable coverage boundary, not a claim that every builtin,
option, CLI command, or future language extension has its own graph node.
Several operations share a family-level compiler owner. Compiler-synthesized
and reserved AST forms are included as representations; their presence does
not establish accepted source syntax or implementation in every backend.

## Ownership and Queries

Each link is curated. Symbol locations are resolved with the Rust parser, so
comments and call sites cannot substitute for declarations. Missing or
ambiguous symbols fail indexing; `Owner::method` can disambiguate methods.
Source review is still needed to establish ownership: symbol existence alone
is not proof that a function implements a feature. A family-level parser,
checker, or emitter may handle several forms rather than only that feature.
No call-graph, semantic equivalence, or compile-success claim is implied.

Examples:

```sh
arch graph dev-query "counter AST parser backend" --root . --index .archdevgraph --token-budget 200 --include-source
arch graph dev-query "register file" --root . --index .archdevgraph --role backend --include-source
arch graph dev-query "ExprKind::ScaledQuantize" --root . --index .archdevgraph --include-source
```

Qualified covered variants are query anchors as well as feature names, aliases,
and mapped symbols. Source paths and generic role words alone do not select a
feature. `--role` filters narrow output; a missing role is not invented. A role
unknown to the entire index is an error; a known role absent from the selected
feature yields no matches. Truncation remains explicit under the byte budget.
Default output puts AST, parser, and backend links ahead of other roles so
bounded source retrieval can retain the common three-part ownership answer.
Use `--role` when investigating a different compiler stage.

Indexing reads each mapped source once and parses each Rust symbol file once,
then resolves its mapped symbols from that cache. Source and map fingerprints,
including the AST used for coverage validation, remain in the index manifest.
Queries reject stale source and changed generator versions.

## Compatibility and Evidence

The `covers` field is optional for custom maps and defaults to empty. Legacy
maps need no `src/ast.rs`. A custom map using coverage must have that file; its
declared variants must exist and be unique, but it need not cover the complete
language. Full inventory completeness is a bundled-map CI contract.

Coverage improves discoverability, not measured token savings by itself. Frozen
benchmark bundles retain their original code, indexes, policies, and results.
Fresh source-bound queries and a search-only ablation are required before any
new graph-efficiency claim. This change does not establish HARC parity.

On experimental candidate `2a2d5c1bf63e9d0fd5093df6df3954ad13f02484`,
the offline router check for "For ARCH counter declarations, locate the AST
declaration type, parser function and SystemVerilog emitter" and its
`register-file` counterpart returned all three requested owners via `dev-query`
without search fallback. Each used one graph command, four counted host reads
(including the index read), and a 307-byte source payload. These counts describe
the bounded host router, not internal graph file I/O or model token usage.
That experimental router is separate from this focused developer-graph change;
these counts are not a measurement of the integrated branch or its default MCP
workflow. See [validation evidence](codegraph_feature_coverage_validation.md)
for the four-trial live rerun and its limitations.
The bundled-map regression also queries every covered variant and checks its
top-ranked owner family and source output within the 200-token byte budget.
