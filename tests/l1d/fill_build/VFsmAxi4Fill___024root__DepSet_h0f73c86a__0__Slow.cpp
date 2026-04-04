// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Design implementation internals
// See VFsmAxi4Fill.h for the primary calling header

#include "VFsmAxi4Fill__pch.h"
#include "VFsmAxi4Fill__Syms.h"
#include "VFsmAxi4Fill___024root.h"

#ifdef VL_DEBUG
VL_ATTR_COLD void VFsmAxi4Fill___024root___dump_triggers__stl(VFsmAxi4Fill___024root* vlSelf);
#endif  // VL_DEBUG

VL_ATTR_COLD void VFsmAxi4Fill___024root___eval_triggers__stl(VFsmAxi4Fill___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Fill___024root___eval_triggers__stl\n"); );
    VFsmAxi4Fill__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    vlSelfRef.__VstlTriggered.set(0U, (IData)(vlSelfRef.__VstlFirstIteration));
#ifdef VL_DEBUG
    if (VL_UNLIKELY(vlSymsp->_vm_contextp__->debug())) {
        VFsmAxi4Fill___024root___dump_triggers__stl(vlSelf);
    }
#endif
}
