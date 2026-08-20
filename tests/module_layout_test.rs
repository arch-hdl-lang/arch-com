//! Guards against merge-conflict resolutions silently re-flattening the P4
//! module splits (incident: PR #770, 2026-08-02).
//!
//! PR #766 (sim_codegen per-construct split) and PR #768 (elaborate
//! directory conversion) were both move-only structural refactors, reviewed
//! and merged on the strength of that guarantee. PR #770 (a large, unrelated
//! feature) resolved its merge conflicts against both by silently taking its
//! own branch's flat file layout while keeping the moved-out semantic
//! content inline -- `src/elaborate.rs` came back as one 16k-line file and
//! `src/sim_codegen/mod.rs` re-absorbed everything #766 had extracted, with
//! nothing in the diff calling out that the *structure* (as opposed to the
//! content) had regressed. `cargo test` was green throughout, because
//! nothing was checking file layout.
//!
//! This test makes that class of regression fail loudly, at the PR that
//! causes it, instead of silently at the next `git log` archaeology session.
//!
//! Extended by PR #771 ("P4 phase 2b") to also guard `src/elaborate/threads.rs`
//! (the generic thread -> FSM lowering family extracted from `elaborate::mod`)
//! -- the same regression class applies to any future split, not just the two
//! that existed when #770 hit.
//!
//! Extended again by the "P4 phase 2c" TLM carve-out to also guard
//! `src/elaborate/tlm.rs` (the `tlm_method` lowering family -- bus-connect
//! sugar, target-thread lowering, initiator-call cohort/arbiter/router/
//! tag-lane synthesis -- extracted from `elaborate::mod`).

use std::path::Path;

fn manifest_path(rel: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

/// `src/elaborate.rs` must NOT exist as a flat file -- it was converted to a
/// directory module (`src/elaborate/mod.rs` + `src/elaborate/params.rs`) by
/// PR #768. A flat `src/elaborate.rs` reappearing means some later change
/// (typically a merge conflict resolution) flattened it back.
#[test]
fn elaborate_is_not_flattened() {
    assert!(
        !manifest_path("src/elaborate.rs").is_file(),
        "src/elaborate.rs exists as a flat file -- PR #768 converted `elaborate` \
         into a directory module (src/elaborate/mod.rs + src/elaborate/params.rs). \
         A flat src/elaborate.rs means a later change (often a merge conflict \
         resolution) silently re-flattened that split. See incident: PR #770, \
         2026-08-02."
    );
}

/// The `elaborate` directory module must have all four of its pieces: the
/// orchestrator (`mod.rs`), the extracted param/override/const-eval family
/// (`params.rs`, PR #768, "P4 phase 2a"), the extracted thread -> FSM
/// lowering family (`threads.rs`, PR #771, "P4 phase 2b"), and the extracted
/// TLM (`tlm_method`) lowering family (`tlm.rs`, "P4 phase 2c").
#[test]
fn elaborate_directory_module_is_present() {
    for rel in [
        "src/elaborate/mod.rs",
        "src/elaborate/params.rs",
        "src/elaborate/threads.rs",
        "src/elaborate/tlm.rs",
    ] {
        assert!(
            manifest_path(rel).is_file(),
            "{rel} is missing -- PR #768's elaborate::params split (param \
             resolution, override application, elaborate-side const-eval, \
             derived-param variant rewriting), PR #771's elaborate::threads \
             split (lower_threads/lower_module_threads, partition_thread_body_*, \
             lock/semaphore/arbiter synthesis, wait-state machinery), and the \
             elaborate::tlm split (lower_tlm_connects/lower_tlm_target_threads/ \
             lower_tlm_initiator_calls, request-arbiter/response-router/tag-lane \
             synthesis) are all expected to live here. A missing file means a \
             later change (typically a merge conflict resolution) silently \
             re-flattened that split back into elaborate::mod -- see incident: \
             PR #770, 2026-08-02, which did exactly this to the splits that \
             existed at the time."
        );
    }
}

/// `src/sim_codegen/mod.rs` must exist (it's the orchestrator + shared
/// types), but must NOT have re-absorbed the content PR #766 extracted from
/// it -- checked indirectly below by requiring every sibling file it split
/// out to still exist as its own file.
#[test]
fn sim_codegen_mod_is_present() {
    assert!(
        manifest_path("src/sim_codegen/mod.rs").is_file(),
        "src/sim_codegen/mod.rs is missing entirely."
    );
}

/// Every sibling file PR #766 ("finish per-construct sim_codegen split — P4
/// phase 1") extracted out of the `sim_codegen` monolith must still exist as
/// its own file. If `src/sim_codegen/mod.rs` re-grows to contain this
/// content instead (as happened when PR #770's merge flattened it), these
/// files disappear -- that's exactly the regression this test exists to
/// catch. See incident: PR #770, 2026-08-02.
#[test]
fn sim_codegen_p4_phase1_siblings_are_present() {
    let siblings = [
        "arbiter.rs",
        "bus_expand.rs",
        "clkgate.rs",
        "collect.rs",
        "const_eval.rs",
        "counter.rs",
        "expr_codegen.rs",
        "functions.rs",
        "pybind.rs",
        "regfile.rs",
        "stmt_codegen.rs",
        "structs.rs",
        "synchronizer.rs",
        "trace.rs",
        "width.rs",
    ];
    for name in siblings {
        let rel = format!("src/sim_codegen/{name}");
        assert!(
            manifest_path(&rel).is_file(),
            "{rel} is missing -- PR #766 (P4 phase 1) extracted this construct \
             emitter / shared-helper module out of src/sim_codegen/mod.rs as a \
             move-only refactor. Its absence means mod.rs has re-absorbed that \
             content (a silent structural flattening -- see incident: PR #770, \
             2026-08-02), or the file was deleted outright."
        );
    }
}

/// The sim_codegen siblings that predate PR #766 (already split out before
/// that phase) should also still be present -- not because #766/#768 touch
/// them, but as a cheap sanity check that this test's own file-existence
/// approach is exercising the real directory and not, say, a stale/empty
/// checkout.
#[test]
fn sim_codegen_pre_existing_siblings_are_present() {
    let siblings = [
        "cam.rs",
        "fifo.rs",
        "fsm.rs",
        "linklist.rs",
        "pipeline.rs",
        "ram.rs",
        "thread_sim.rs",
    ];
    for name in siblings {
        let rel = format!("src/sim_codegen/{name}");
        assert!(
            manifest_path(&rel).is_file(),
            "{rel} is missing. This predates the P4 phase-1 split and should \
             always be present; its absence suggests this test is looking at \
             the wrong directory rather than a P4-split regression."
        );
    }
}
