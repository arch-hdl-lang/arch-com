# Compiler Ownership Inventory

This is a link-cardinality audit of all 73 bundled feature families.
Only 25 have exactly one symbol link for each of AST, parser and backend.
That count is not semantic proof, query readiness, compilation coverage or a
measured token saving. All mapped symbols are separately resolved by Rust
syntax parsing during index tests. Missing backend links remain missing rather
than borrowing an unrelated routine to fill the table.

- `missing`: no link for this role.
- `file-only`: one file-level link without a declaration witness.
- `one-symbol`: exactly one declaration-symbol link.
- `multiple(N)`: N links; a single-answer workflow must disambiguate them.

The Rust test `all_feature_ast_parser_backend_links_are_audited_by_cardinality`
checks this table against the map, so additions or ownership changes require
updating this inventory. Run it with:

```sh
cargo test --lib all_feature_ast_parser_backend_links_are_audited_by_cardinality -- --nocapture
```

| Feature | AST | Parser | Backend |
| --- | --- | --- | --- |
| module | one-symbol | one-symbol | one-symbol |
| bus | one-symbol | one-symbol | file-only |
| bus handshake metadata | one-symbol | one-symbol | one-symbol |
| bus credit channel metadata | one-symbol | one-symbol | multiple(3) |
| credit channel sender state | one-symbol | one-symbol | one-symbol |
| credit channel receiver state | one-symbol | one-symbol | one-symbol |
| credit channel protocol assertions | one-symbol | one-symbol | one-symbol |
| bus conditional generation | one-symbol | one-symbol | missing |
| parameters and local parameters | one-symbol | one-symbol | one-symbol |
| ports and registered ports | multiple(3) | one-symbol | missing |
| fifo | one-symbol | one-symbol | multiple(2) |
| thread | one-symbol | one-symbol | file-only |
| fsm | one-symbol | one-symbol | one-symbol |
| pipeline | one-symbol | one-symbol | one-symbol |
| stage | one-symbol | one-symbol | missing |
| stall directive | one-symbol | one-symbol | missing |
| flush directive | one-symbol | one-symbol | missing |
| forwarding directive | one-symbol | one-symbol | missing |
| pipe_reg declaration | one-symbol | one-symbol | one-symbol |
| pipe_reg output-port reset | one-symbol | one-symbol | one-symbol |
| tlm_method | one-symbol | one-symbol | one-symbol |
| arbiter | one-symbol | one-symbol | multiple(2) |
| clock domain | one-symbol | one-symbol | one-symbol |
| struct | one-symbol | one-symbol | one-symbol |
| enum | one-symbol | one-symbol | one-symbol |
| RAM | one-symbol | one-symbol | one-symbol |
| CAM | one-symbol | one-symbol | one-symbol |
| counter | one-symbol | one-symbol | one-symbol |
| register file | one-symbol | one-symbol | one-symbol |
| synchronizer | one-symbol | one-symbol | one-symbol |
| clock gate | one-symbol | one-symbol | one-symbol |
| function | one-symbol | one-symbol | one-symbol |
| linklist | one-symbol | one-symbol | one-symbol |
| template | one-symbol | one-symbol | missing |
| template hooks | one-symbol | one-symbol | missing |
| module hooks | one-symbol | one-symbol | missing |
| arbiter hooks | one-symbol | one-symbol | missing |
| package | one-symbol | one-symbol | missing |
| use declaration | one-symbol | one-symbol | missing |
| external package | one-symbol | one-symbol | missing |
| generate | multiple(2) | multiple(2) | one-symbol |
| conditional generate | one-symbol | one-symbol | one-symbol |
| iterative generate | one-symbol | one-symbol | one-symbol |
| register declaration | one-symbol | one-symbol | missing |
| alias declaration | one-symbol | one-symbol | missing |
| wire declaration | one-symbol | one-symbol | missing |
| let binding | one-symbol | one-symbol | missing |
| combinational block | one-symbol | one-symbol | missing |
| sequential block | one-symbol | one-symbol | missing |
| latch block | one-symbol | one-symbol | missing |
| instance | one-symbol | one-symbol | missing |
| port arrays | one-symbol | one-symbol | missing |
| assertion and coverage | one-symbol | one-symbol | one-symbol |
| TLM connect | one-symbol | one-symbol | missing |
| resource lock | one-symbol | one-symbol | missing |
| procedural statements | one-symbol | one-symbol | missing |
| scalar and aggregate types | one-symbol | one-symbol | missing |
| floating-point and block types | one-symbol | one-symbol | missing |
| expression forms | one-symbol | one-symbol | missing |
| expression selection and casts | one-symbol | one-symbol | missing |
| expression matches and conditionals | one-symbol | one-symbol | missing |
| expression operators | one-symbol | one-symbol | missing |
| expression calls and helpers | one-symbol | one-symbol | missing |
| scaled quantization expression | one-symbol | one-symbol | missing |
| latency and SVA expressions | one-symbol | one-symbol | missing |
| arithmetic binary operators | one-symbol | one-symbol | missing |
| comparison binary operators | one-symbol | one-symbol | missing |
| logical and bitwise binary operators | one-symbol | one-symbol | missing |
| shift binary operators | one-symbol | one-symbol | missing |
| wrapping arithmetic operators | one-symbol | one-symbol | missing |
| temporal implication operators | one-symbol | one-symbol | missing |
| unary logical and arithmetic operators | one-symbol | one-symbol | missing |
| unary bitwise operators | one-symbol | one-symbol | missing |

These categories describe explicit curated links, not inferred completeness.
Some families share helpers; others lower into a different construct or have no
standalone emitter. Completing those mappings requires source review. The new
conditional and iterative generate families preserve the generic family's
ambiguity rather than choosing its first parser.
