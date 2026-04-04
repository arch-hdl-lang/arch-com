// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Design implementation internals
// See VFsmCacheCtrl.h for the primary calling header

#include "VFsmCacheCtrl__pch.h"
#include "VFsmCacheCtrl__Syms.h"
#include "VFsmCacheCtrl___024root.h"

#ifdef VL_DEBUG
VL_ATTR_COLD void VFsmCacheCtrl___024root___dump_triggers__ico(VFsmCacheCtrl___024root* vlSelf);
#endif  // VL_DEBUG

void VFsmCacheCtrl___024root___eval_triggers__ico(VFsmCacheCtrl___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmCacheCtrl___024root___eval_triggers__ico\n"); );
    VFsmCacheCtrl__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    vlSelfRef.__VicoTriggered.set(0U, (IData)(vlSelfRef.__VicoFirstIteration));
#ifdef VL_DEBUG
    if (VL_UNLIKELY(vlSymsp->_vm_contextp__->debug())) {
        VFsmCacheCtrl___024root___dump_triggers__ico(vlSelf);
    }
#endif
}

#ifdef VL_DEBUG
VL_ATTR_COLD void VFsmCacheCtrl___024root___dump_triggers__act(VFsmCacheCtrl___024root* vlSelf);
#endif  // VL_DEBUG

void VFsmCacheCtrl___024root___eval_triggers__act(VFsmCacheCtrl___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmCacheCtrl___024root___eval_triggers__act\n"); );
    VFsmCacheCtrl__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    vlSelfRef.__VactTriggered.set(0U, ((IData)(vlSelfRef.clk) 
                                       & (~ (IData)(vlSelfRef.__Vtrigprevexpr___TOP__clk__0))));
    vlSelfRef.__Vtrigprevexpr___TOP__clk__0 = vlSelfRef.clk;
#ifdef VL_DEBUG
    if (VL_UNLIKELY(vlSymsp->_vm_contextp__->debug())) {
        VFsmCacheCtrl___024root___dump_triggers__act(vlSelf);
    }
#endif
}
