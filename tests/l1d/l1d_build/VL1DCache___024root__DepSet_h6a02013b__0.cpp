// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Design implementation internals
// See VL1DCache.h for the primary calling header

#include "VL1DCache__pch.h"
#include "VL1DCache__Syms.h"
#include "VL1DCache___024root.h"

#ifdef VL_DEBUG
VL_ATTR_COLD void VL1DCache___024root___dump_triggers__ico(VL1DCache___024root* vlSelf);
#endif  // VL_DEBUG

void VL1DCache___024root___eval_triggers__ico(VL1DCache___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VL1DCache___024root___eval_triggers__ico\n"); );
    VL1DCache__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    vlSelfRef.__VicoTriggered.set(0U, (IData)(vlSelfRef.__VicoFirstIteration));
#ifdef VL_DEBUG
    if (VL_UNLIKELY(vlSymsp->_vm_contextp__->debug())) {
        VL1DCache___024root___dump_triggers__ico(vlSelf);
    }
#endif
}

#ifdef VL_DEBUG
VL_ATTR_COLD void VL1DCache___024root___dump_triggers__act(VL1DCache___024root* vlSelf);
#endif  // VL_DEBUG

void VL1DCache___024root___eval_triggers__act(VL1DCache___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VL1DCache___024root___eval_triggers__act\n"); );
    VL1DCache__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    vlSelfRef.__VactTriggered.set(0U, ((IData)(vlSelfRef.clk) 
                                       & (~ (IData)(vlSelfRef.__Vtrigprevexpr___TOP__clk__0))));
    vlSelfRef.__Vtrigprevexpr___TOP__clk__0 = vlSelfRef.clk;
#ifdef VL_DEBUG
    if (VL_UNLIKELY(vlSymsp->_vm_contextp__->debug())) {
        VL1DCache___024root___dump_triggers__act(vlSelf);
    }
#endif
}
