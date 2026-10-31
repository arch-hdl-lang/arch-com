import ArchCodegenProof.Core
import ArchCodegenProof.Sv

/-!
# The codegen validator and its soundness theorem

`check core sv` is a *decidable* function. A compiler-emitted certificate
replays it on the concrete ARCH-Core module and the concrete SV module
read back from `arch build` output, closing `check core sv = true` by kernel
evaluation (`decide`). `check_sound` then yields, with no further trusted
reasoning:

* the two modules have the same port list (names, directions, widths),
* their flops are clocked by the same port, and
* **every trace of the ARCH module is a trace of the SV module and vice
  versa**, for all input streams and every cycle.
-/

namespace Arch.CodegenProof

def check (c : Core.Module) (s : Sv.Module) : Bool :=
  decide (c.ports = s.ports) && decide (c.clk = s.ffClk) && s.supported && s.asyncShaped &&
    -- Initializers only on clocked variables: one on a combinational net
    -- could make the run set empty, and empty run sets are vacuously equal.
    Netlist.regSubB (s.toNetlist.inits.map Prod.fst) s.toNetlist.regs &&
    Netlist.sameB c.toNetlist s.toNetlist

theorem check_sound {c : Core.Module} {s : Sv.Module} (h : check c s = true) :
    c.ports = s.ports ∧ c.clk = s.ffClk ∧ ∀ tr, c.Run tr ↔ s.Run tr := by
  simp only [check, Bool.and_eq_true, decide_eq_true_eq] at h
  obtain ⟨⟨⟨⟨⟨hp, hc⟩, hs⟩, _⟩, _⟩, hn⟩ := h
  refine ⟨hp, hc, fun tr => ?_⟩
  rw [Core.Module.run_iff, Sv.Module.run_iff s hs, Netlist.run_iff_of_sameB hn]

end Arch.CodegenProof
