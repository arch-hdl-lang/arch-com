//! C++ simulation emission for `credit_channel` bus sub-constructs.
//!
//! Split out of `sim_codegen.rs` (which is already ~7400 LoC) to keep the
//! credit-channel state machinery reviewable on its own. Mirrors the SV
//! emission in `codegen::emit_credit_channel_state` /
//! `codegen::emit_credit_channel_receiver_state` /
//! `codegen::emit_credit_channel_asserts`, but for the pybind11 /
//! cocotb-shim simulation path.
//!
//! Hook points used by `sim_codegen`:
//!
//! * `collect_credit_channels(m, symbols)` — gather the per-port credit
//!   channels the module touches and classify each as sender/receiver.
//!   Returned data structure drives field declarations and update logic.
//! * `emit_header_fields(...)` — inject private C++ fields for the sender
//!   counter, receiver FIFO, and the derived `can_send` / `valid` / `data`
//!   signals.
//! * `emit_constructor_inits(...)` — zero-initialize all synthesized fields.
//! * `emit_posedge_updates(...)` — mirror the SV `always_ff` semantics:
//!   sender-side counter update and receiver-side FIFO push/pop.
//!
//! Scope: this module handles only the `module` emission path today. Other
//! constructs that can carry bus ports (pipeline, thread, arbiter, etc.)
//! will hook in as their own eval_posedge emitters are extended.

use crate::ast::{BusPerspective, CreditChannelMeta, Direction, ModuleDecl, TypeExpr};
use crate::resolve::{Symbol, SymbolTable};

/// Per-port, per-channel record classifying this module's role on each
/// credit_channel it touches. Populated from the module's bus ports and
/// the matching `BusInfo.credit_channels` metadata.
#[derive(Debug, Clone)]
pub struct CreditChannelSite {
    pub port_name: String,
    pub channel: CreditChannelMeta,
    pub role: CreditChannelRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreditChannelRole {
    Sender,
    Receiver,
}

/// Walk the module's ports and classify each credit_channel on each bus
/// port. Empty result short-circuits the caller — no fields / no update.
pub fn collect_credit_channels(m: &ModuleDecl, symbols: &SymbolTable) -> Vec<CreditChannelSite> {
    let mut out = Vec::new();
    for p in &m.ports {
        let Some(ref bi) = p.bus_info else { continue; };
        let Some((Symbol::Bus(info), _)) = symbols.globals.get(&bi.bus_name.name) else { continue; };
        for cc in &info.credit_channels {
            let role = match (cc.role_dir, bi.perspective) {
                (Direction::Out, BusPerspective::Initiator)
                | (Direction::In, BusPerspective::Target)    => CreditChannelRole::Sender,
                _                                             => CreditChannelRole::Receiver,
            };
            out.push(CreditChannelSite {
                port_name: p.name.name.clone(),
                channel: cc.clone(),
                role,
            });
        }
    }
    out
}

// Emitter stubs — bodies land in the upcoming focused PRs. Leaving them as
// `todo!`-style no-ops would silently drop behavior; instead the callers
// simply won't invoke them until we wire each up.

/// Append C++ private field declarations for each site to the header
/// buffer. Matches the SV names emitted by codegen so `SynthIdent`
/// references resolve to real symbols.
pub fn emit_header_fields(_sites: &[CreditChannelSite], _h: &mut String) {
    // PR-sim-1 (sender) / PR-sim-2 (receiver) land here.
}

/// Append C++ constructor zero-initializers for each site to the buffer.
pub fn emit_constructor_inits(_sites: &[CreditChannelSite], _cpp: &mut String) {
    // PR-sim-1 / PR-sim-2 land here.
}

/// Append C++ `eval_posedge` update logic for each site to the buffer.
/// `rst_active` is the C++ expression that evaluates to 1 when the
/// module's reset is active (or `None` if the module has no reset port).
pub fn emit_posedge_updates(
    _sites: &[CreditChannelSite],
    _rst_active: Option<&str>,
    _cpp: &mut String,
) {
    // PR-sim-1 / PR-sim-2 land here.
}

/// Convenience: C++ type for the payload type of a credit_channel.
/// Uses the declared `T` param default; no port-site override.
pub fn payload_cpp_type(cc: &CreditChannelMeta) -> Option<TypeExpr> {
    cc.params.iter()
        .find(|p| p.name.name == "T")
        .and_then(|p| match &p.kind {
            crate::ast::ParamKind::Type(te) => Some(te.clone()),
            _ => None,
        })
}
