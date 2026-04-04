// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Design implementation internals
// See VFsmAxi4Fill.h for the primary calling header

#include "VFsmAxi4Fill__pch.h"
#include "VFsmAxi4Fill__Syms.h"
#include "VFsmAxi4Fill___024root.h"

#ifdef VL_DEBUG
VL_ATTR_COLD void VFsmAxi4Fill___024root___dump_triggers__ico(VFsmAxi4Fill___024root* vlSelf);
#endif  // VL_DEBUG

void VFsmAxi4Fill___024root___eval_triggers__ico(VFsmAxi4Fill___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Fill___024root___eval_triggers__ico\n"); );
    VFsmAxi4Fill__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    vlSelfRef.__VicoTriggered.set(0U, (IData)(vlSelfRef.__VicoFirstIteration));
#ifdef VL_DEBUG
    if (VL_UNLIKELY(vlSymsp->_vm_contextp__->debug())) {
        VFsmAxi4Fill___024root___dump_triggers__ico(vlSelf);
    }
#endif
}

#ifdef VL_DEBUG
VL_ATTR_COLD void VFsmAxi4Fill___024root___dump_triggers__act(VFsmAxi4Fill___024root* vlSelf);
#endif  // VL_DEBUG

void VFsmAxi4Fill___024root___eval_triggers__act(VFsmAxi4Fill___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Fill___024root___eval_triggers__act\n"); );
    VFsmAxi4Fill__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    vlSelfRef.__VactTriggered.set(0U, ((IData)(vlSelfRef.clk) 
                                       & (~ (IData)(vlSelfRef.__Vtrigprevexpr___TOP__clk__0))));
    vlSelfRef.__Vtrigprevexpr___TOP__clk__0 = vlSelfRef.clk;
#ifdef VL_DEBUG
    if (VL_UNLIKELY(vlSymsp->_vm_contextp__->debug())) {
        VFsmAxi4Fill___024root___dump_triggers__act(vlSelf);
    }
#endif
}
