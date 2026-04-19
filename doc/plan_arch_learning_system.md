# Plan: ARCH Compiler Learning System

*Author: session of 2026-04-18. Status: long-term plan; v1 scope is implementable now.*

## Motivation

ARCH is designed to be generated correctly by LLM agents. Each time a user or agent interacts with the compiler — hitting an error, fixing it, writing code that compiles cleanly — information is produced that could make subsequent LLM runs more accurate. Today that information is discarded. This plan defines a local-first learning system that captures it, makes it retrievable by agents, and (optionally) lets contributors share curated subsets to build an open training corpus for the ARCH ecosystem.

Privacy constraint: default behavior is strictly local. Nothing leaves the machine without explicit contributor action.

## Goals

1. **Capture**: record compiler interactions (errors, fixes, successful compiles, verification outcomes) to a local store.
2. **Retrieve**: when an LLM agent hits an error or is asked to generate new code, let it query the store for relevant past examples.
3. **Promote**: when a pattern stabilizes in the store, encode it as a compiler lint — graduating RAG memory to compile-time enforcement.
4. **Contribute** (optional, with consent): let users submit curated snippets to a shared public corpus that benefits the whole community and produces open training data for the next generation of ARCH-capable models.

## Non-goals

- Silent telemetry. All capture and submission is under explicit user control.
- Uploading raw project files. Only hand-curated, contributor-labeled snippets ever reach any shared corpus.
- Training foundation models in-house. The corpus is an open dataset; actual LLM training is someone else's problem.
- Replacing the spec. The spec is authoritative; the learning store is *empirical* and may contain past mistakes.

## Techniques, from lowest to highest leverage

| Technique | What it captures | Leverage |
|---|---|---|
| Compiler lints | Universal anti-patterns | 🔥🔥🔥 Every user, zero runtime cost — enforce at compile time |
| Memory file (`CLAUDE.md`-style) | User/project rules | 🔥🔥 Always-in-context; few bytes |
| RAG over error→fix pairs | Novel errors and their resolutions | 🔥 Retrievable few-shot examples |
| RAG over idiom corpus | Construct composition examples | 🔥 New code mirrors existing style |
| Fine-tuning / LoRA | Deep pattern absorption | Only for local open-weight models |

Rule of thumb: start with RAG (low commitment, high value), and **promote stable patterns to lints** so the corpus stays focused on novel/transient knowledge.

## What's worth capturing

### Tier 1 — Highest signal, easiest to capture

**Compile error → fix pairs.** Trigger: `arch check` fails → user edits → next `arch check` on the same file succeeds. Store: `(error_code, error_message, error_span_context, src_before, src_after, minimal_diff)`.

### Tier 2 — Medium signal, moderate capture cost

**Construct composition idioms.** When a user successfully compiles a design using ≥2 first-class constructs together (FSM + FIFO, pipeline + arbiter, thread + synchronizer), the AST combination is a shareable idiom. Trigger: `arch build` successful on a file containing N≥2 distinct construct kinds with non-trivial connections.

**Verification failures + root causes.** Trigger: EBMC refutation → user fix → re-run passes. Store: `(property, counterexample_summary, src_before, src_after)`.

### Tier 3 — Highest value for LLM training, hardest to capture

**Natural-language prompt → final ARCH code.** When an LLM agent generates ARCH in response to an English prompt and the user accepts the result (compiles clean + tests pass + code is committed), the pair is literal training data. Capture requires editor/agent integration — record the prompt and the accepted AST.

## Architecture (end state)

```
┌─────────────────────────────────────────────────────────────────┐
│                    Local to the user's machine                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌───────────────┐     ┌──────────────────────────────┐        │
│  │ arch compiler │────▶│ ~/.arch/learn/               │        │
│  │ check/build/  │     │  ├── events.jsonl            │        │
│  │ sim/formal    │     │  │     (raw capture stream)  │        │
│  └───────────────┘     │  ├── index/                  │        │
│          ▲             │  │     (BM25/TF-IDF or       │        │
│          │             │  │      vector embeddings)   │        │
│          │             │  └── pending/                │        │
│          │             │        (in-flight failures)  │        │
│          │             └──────────────────────────────┘        │
│          │                           ▲                          │
│          │                           │                          │
│          │                ┌──────────────────────┐             │
│          └────tool call───│  arch advise <query> │             │
│                           └──────────────────────┘             │
│                                       ▲                         │
│                                       │                         │
│                           ┌──────────────────────┐             │
│                           │  Claude Code agent   │             │
│                           │  (or any LLM)        │             │
│                           └──────────────────────┘             │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│                    Explicit contributor action                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   arch contribute          ──▶ interactive review + submit     │
│                                per-snippet                     │
│                                       │                        │
│                                       ▼                        │
│           ┌───────────────────────────────────────┐            │
│           │  github.com/arch-hdl-lang/training-   │            │
│           │  corpus  (CC0, open dataset, PR-based)│            │
│           └───────────────────────────────────────┘            │
└─────────────────────────────────────────────────────────────────┘
```

## Roadmap

### v1 — minimum useful local loop (SHIPPED in v0.42.0)

Delivered in commits on branch `learn` (PR #17).

- **Always-on capture.** Every `arch check`/`build`/`sim`/`formal` invocation records error→fix pairs into `~/.arch/learn/events.jsonl`. No flag required; `ARCH_NO_LEARN=1` opts out.
  - On failure: stash `(file_path, src, error_code, error_message, timestamp)` into `~/.arch/learn/pending/<file_hash>.json`.
  - On success with a pending entry: pair into an `error_fix` event, append to `events.jsonl`, print `📚 Learned: [<code>] <diff>`.
  - Event schema:
    ```json
    {
      "ts": "2026-04-18T20:00:00Z",
      "kind": "error_fix",
      "error_code": "width_mismatch",
      "error_message": "RHS is UInt<9> but LHS is UInt<8>",
      "file_path": "MyMod.arch",
      "src_before": "<full file text before fix>",
      "src_after": "<full file text after fix>",
      "diff_summary": "cnt <= cnt + 1;  →  cnt <= (cnt + 1).trunc<8>();"
    }
    ```
- **Size cap.** 100 MB default (override `ARCH_LEARN_MAX_MB`). Warns once at ≥90% full; hard-skips writes at 100%.
- **`arch learn-index`** — BM25 lexical index over error_code + error_message + diff_summary. Pure Rust; no new deps.
- **`arch advise <query> [-k N]`** — top-K retrieval with score, code, message, file, diff.
- **`arch learn-stats`** — event count + breakdown by error_code.
- **`arch learn-clear`** — wipe the whole store.
- **`arch learn-prune --code|--contains|--older-than-days [--dry-run]`** — remove events by filter; auto-invalidates the index.
- **MCP tools**: `arch_advise`, `arch_learn_index`, `arch_learn_stats`, `arch_learn_prune` exposed to agents. Server instructions tell agents to call `arch_advise` on every compile error before attempting a fix.
- **Retroactive backfill**: a one-off Python miner (`/tmp/mine_learn.py`, not in repo) parses Claude Code session logs and extracts fail→edit→success triples into the store. Used to seed ~44 events across 9 historical sessions.

JSONL is the source of truth; BM25 `index.json` is disposable and rebuilt on demand. No external embedding model, no network, no telemetry, single static compiler binary.

### v1.1 — quality-of-life (partial)

- [SHIPPED] `arch learn-stats`, `arch learn-clear`, `arch learn-prune`
- [TODO] `arch advise --from-stderr` — pipe the latest compiler error into advise without copying.
- [TODO] Auto-suggestion hook: `arch check` prints "💡 `arch advise` found 2 similar past errors" when the store has relevant entries for the current failure.
- [TODO] **Usage counters.** Add `retrieved_count` (bumped on every `advise` top-K hit) and `helped_count` (bumped when a compile-clean follows an advise within the same session). These become the downstream-utility signal for the data-coop ranker (see `plan_arch_data_coop.md`, §3.2). Step 1 (retrieved_count) is cheap; step 2 needs a per-session nonce written to pending state.

### v2 — richer capture + semantic retrieval

**Capture expansions** (orthogonal to retrieval backend):
- Record **successful compiles** of multi-construct designs → idiom corpus.
- Record **verification failures** (EBMC, SVA) → root-cause corpus.
- Record **prompt → code** pairs (requires editor/agent integration; new `arch learn-prompt` API).

**Retrieval upgrade: SQLite + sqlite-vec + local ONNX embeddings.** Deferred until the lexical BM25 v1 shows concrete limits — volume or semantic-miss symptoms. Design when we pick it up:

| Decision | Choice | Rationale |
|---|---|---|
| Vector DB | `sqlite-vec` via `rusqlite` | One file under `~/.arch/learn/store.db`; preserves "nothing leaves the machine" story |
| Embedding backend | Pure-Rust ONNX (`ort` + `tokenizers`) | No Python dep; no bundled 1 GB venv; runs offline after first-use download |
| Model | `all-MiniLM-L6-v2` (384-dim, ~90 MB) as default; offer `bge-code-v1.5` (1024-dim, code-specialized, ~340 MB) as opt-in | MiniLM is fast enough for real-time `advise`; code-specialized is a later tuning knob |
| Model storage | `~/.arch/learn/model/` — auto-download on first `arch learn-index` | Compiler binary stays small; one-time cost |
| API-based embeddings | **Rejected** | Would send source off-device; violates always-local privacy rule |
| Migration | Rebuild `store.db` from `events.jsonl`; keep JSONL as source of truth | `events.jsonl` stays human-inspectable; prune/clear stay trivial |
| Reranking | Embed query → top-50 cosine candidates → BM25 rerank | Fuses semantic recall with exact-term precision |
| Scale test | New `arch learn-bench` synthesizes N events and reports query latency at N = 100/1K/10K/100K | Concrete evidence for when the v2 investment pays off |

**Exit criteria for triggering v2 work**: either (a) single-user store passes ~1,000 events and `advise` top-3 starts feeling stale, or (b) queries with no lexical overlap (e.g. "too-wide assignment" vs. a stored "width mismatch") consistently miss good matches. Until then, BM25 is adequate and cheaper on every axis.

### v3 — contributor sharing

- **`arch contribute`** interactive CLI:
  1. Reads local events store.
  2. Shows each eligible event as a redacted diff.
  3. User approves per-item (or bulk).
  4. Bundles approved items into a PR against `arch-hdl-lang/training-corpus`.

- **Consent mechanism**: extend CLA with a section granting license to submitted training data under CC0 (or similar). Or require a Git trailer `Training-Data-Consent: yes` on commits whose snippets may be submitted.

- **Automated scrubbing**: regex + entropy-based scan for API keys, tokens, emails, PII before anything is submitted.

- **Corpus repo**: `arch-hdl-lang/training-corpus` organized as:
  ```
  training-corpus/
    ├── README.md         # consent terms, license, contribution process
    ├── errors/           # error→fix pairs, grouped by error code
    ├── idioms/           # construct composition examples
    ├── prompts/          # NL → ARCH pairs (most sensitive, tightest consent)
    └── verification/     # formal/simulation failures and fixes
  ```
  Each entry has YAML front-matter: `contributor`, `license`, `consent_commit`, `scrub_status`.

- **Moderation**: review team approves incoming PRs for the first quarter of operation until automation is trusted.

### v4 — promotion loop

- Periodic "lint promotion" pass: identify error_codes appearing ≥N times across the corpus. Draft a compiler lint that statically rejects the anti-pattern. Ship as a new compiler version. Mark the corresponding corpus entries as "graduated" (kept for historical training value, no longer shown by `arch advise` since the compiler handles it).

- Feedback loop: compiler version N's lints are trained from corpus version N-1, and corpus N gains data from version N users. Each release makes the compiler stricter and the remaining corpus more specialized (edge cases, style preferences, project-specific patterns).

## Privacy + consent — the hard part

Even though v1 is fully local, users must trust that:

1. `arch check` without `--learn` writes nothing to disk beyond normal compiler output.
2. `arch check --learn` only writes to `~/.arch/learn/`, never transmits over the network.
3. `arch advise` only reads locally; no network activity.
4. `arch contribute` (v3) requires explicit per-item approval and never auto-submits.

Implementation rules:
- All `--learn` data lives under `~/.arch/learn/`, never in the project tree.
- `arch check --learn` is opt-in per invocation. Editor integrations may default it on, but the base CLI never does.
- A lint-style warning on first `arch check --learn` run: "📚 Learning mode is ON. Data will be stored at ~/.arch/learn/. Run `arch learn stats` to inspect, `arch learn clear` to delete. Nothing is shared off-device unless you explicitly run `arch contribute`."
- No analytics, no telemetry, no phoning home. Ever. If we add it later (say, for crash reports), it's a separate explicit flag with different defaults and different documentation.

## Legal considerations (for v3+)

When a contributor runs `arch contribute`, they need to agree to:

1. **Copyright warranty**: "I own the code I'm submitting, or have rights to license it."
2. **License grant**: submissions are CC0 (or CC-BY) — maximally reusable.
3. **Revocation**: contributor can request deletion (GDPR right to erasure); revocation removes the entry from the corpus and flags it as deleted in future dataset releases.
4. **No secrets**: contributor warrants they've reviewed and scrubbed any sensitive material.

The CLA that already gates PR contribution can be extended with a "Training Data Consent" section, opt-in (default off). Or create a separate `TRAINING_DATA_CLA.md` that contributors sign once per account.

## Governance

- **Review team**: small group (2-3 people) approves incoming `training-corpus` PRs for the first 3 months, until automated scrubbing is trusted.
- **Transparency reports**: quarterly stats on the training-corpus repo: N contributors, N examples, N redactions required, top error codes, graduation count (entries → compiler lints).
- **Open dataset**: the corpus is public, auditable, and licensed openly. It's not a private asset sold or licensed to any specific LLM vendor — it's an open training resource available to everyone.

## v1 implementation summary

Files to add/modify:
- `src/main.rs`: add `--learn` flag to `Check`, add `LearnIndex` and `Advise` subcommands
- `src/learn.rs`: new module with:
  - `record_failure(file, err_code, err_msg, span, src) -> Result<()>`
  - `record_success_if_pending(file, src) -> Result<Option<Event>>`
  - `build_index() -> Result<()>`
  - `advise(query, k) -> Result<Vec<MatchedEvent>>`
- `Cargo.toml`: add `dirs` crate for `~/.arch/` path resolution, `serde_json` already present

Tests:
- Unit test for BM25 scoring on a canned events set
- Integration test: run `arch check --learn` on a failing file, fix it, run again, verify event was recorded
- Integration test: run `arch advise "width mismatch"` on a pre-populated store, verify top-K makes sense

Data paths: `~/.arch/learn/events.jsonl`, `~/.arch/learn/pending/<hash>.json`, `~/.arch/learn/index.json`.

Privacy: prints a one-time first-run notice.
