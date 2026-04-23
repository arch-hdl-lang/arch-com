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

/// Append C++ private field declarations for each site.
///
/// Sender emits:
///   uint32_t __<port>_<ch>_credit;
///   uint8_t  __<port>_<ch>_can_send;
///
/// Receiver FIFO fields land in the receiver-side slice (PR-sim-2).
pub fn emit_header_fields(sites: &[CreditChannelSite], h: &mut String) {
    for s in sites {
        match s.role {
            CreditChannelRole::Sender => {
                let port = &s.port_name;
                let ch = &s.channel.name.name;
                h.push_str(&format!("  uint32_t __{port}_{ch}_credit;\n"));
                h.push_str(&format!("  uint8_t  __{port}_{ch}_can_send;\n"));
            }
            CreditChannelRole::Receiver => {
                // PR-sim-2 — FIFO buffer, head/tail/occupancy, valid/data.
            }
        }
    }
}

/// Append C++ constructor zero-initializers for each site. Runs as a
/// constructor-body fragment (not an init list) — each line is a plain
/// `field = 0;` statement.
pub fn emit_constructor_inits(sites: &[CreditChannelSite], cpp: &mut String) {
    for s in sites {
        match s.role {
            CreditChannelRole::Sender => {
                let port = &s.port_name;
                let ch = &s.channel.name.name;
                // Initial values match the SV reset semantics:
                //   credit = DEPTH, can_send = (DEPTH != 0).
                // DEPTH is a const param; we resolve its literal value
                // below when we have one, else fall through to 0.
                let depth = depth_literal(&s.channel).unwrap_or(0);
                cpp.push_str(&format!("  __{port}_{ch}_credit = {depth};\n"));
                cpp.push_str(&format!("  __{port}_{ch}_can_send = {};\n",
                    if depth != 0 { 1 } else { 0 }));
            }
            CreditChannelRole::Receiver => {}
        }
    }
}

/// Append C++ `eval_posedge` update logic for each site. Mirrors the SV
/// `always_ff` emitted by codegen:
///
///   if (rst_active) { credit = DEPTH; can_send = (DEPTH != 0); }
///   else if ( send_valid && !credit_return) credit--;
///   else if (!send_valid &&  credit_return) credit++;
///   (and can_send is re-derived in eval_comb — see emit_comb_updates)
///
/// `rst_active` is a C++ boolean expression that is true while reset is
/// asserted. `None` means the module has no reset port; the reset branch
/// is suppressed.
pub fn emit_posedge_updates(
    sites: &[CreditChannelSite],
    rst_active: Option<&str>,
    cpp: &mut String,
) {
    for s in sites {
        match s.role {
            CreditChannelRole::Sender => {
                let port = &s.port_name;
                let ch = &s.channel.name.name;
                let credit = format!("__{port}_{ch}_credit");
                let send_valid = format!("{port}_{ch}_send_valid");
                let credit_ret = format!("{port}_{ch}_credit_return");
                let depth = depth_literal(&s.channel).unwrap_or(0);
                cpp.push_str("  {\n");
                if let Some(r) = rst_active {
                    cpp.push_str(&format!("    if ({r}) {{ {credit} = {depth}; }}\n"));
                    cpp.push_str(&format!("    else if ({send_valid} && !{credit_ret}) {credit}--;\n"));
                    cpp.push_str(&format!("    else if (!{send_valid} &&  {credit_ret}) {credit}++;\n"));
                } else {
                    cpp.push_str(&format!("    if ({send_valid} && !{credit_ret}) {credit}--;\n"));
                    cpp.push_str(&format!("    else if (!{send_valid} &&  {credit_ret}) {credit}++;\n"));
                }
                cpp.push_str("  }\n");
            }
            CreditChannelRole::Receiver => {}
        }
    }
}

/// Append C++ `eval_comb` updates for each site. Handles the
/// combinational `can_send` wire (and, for receivers, the `valid` /
/// `data` wires — PR-sim-2).
pub fn emit_comb_updates(sites: &[CreditChannelSite], cpp: &mut String) {
    for s in sites {
        match s.role {
            CreditChannelRole::Sender => {
                let port = &s.port_name;
                let ch = &s.channel.name.name;
                // Combinational can_send. If CAN_SEND_REGISTERED=1, this
                // assignment is overridden at eval_posedge time (register
                // holds its value between edges); keeping the comb
                // assignment is still safe — it just recomputes what the
                // flop already holds.
                cpp.push_str(&format!(
                    "  __{port}_{ch}_can_send = (__{port}_{ch}_credit != 0) ? 1 : 0;\n"
                ));
            }
            CreditChannelRole::Receiver => {}
        }
    }
}

/// Resolve the channel's DEPTH param to an integer literal if possible.
/// Returns None for non-literal expressions (param references etc.); the
/// caller falls back to 0 (zero-init), matching the SV behavior of
/// leaving the counter at DEPTH evaluated from the port-site param map.
fn depth_literal(cc: &CreditChannelMeta) -> Option<u64> {
    use crate::ast::{ExprKind, LitKind};
    cc.params.iter()
        .find(|p| p.name.name == "DEPTH")
        .and_then(|p| p.default.as_ref())
        .and_then(|e| match &e.kind {
            ExprKind::Literal(LitKind::Dec(v))
            | ExprKind::Literal(LitKind::Hex(v))
            | ExprKind::Literal(LitKind::Bin(v))
            | ExprKind::Literal(LitKind::Sized(_, v)) => Some(*v),
            _ => None,
        })
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
