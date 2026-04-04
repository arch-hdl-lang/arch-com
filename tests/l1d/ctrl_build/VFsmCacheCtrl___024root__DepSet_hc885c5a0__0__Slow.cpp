// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Design implementation internals
// See VFsmCacheCtrl.h for the primary calling header

#include "VFsmCacheCtrl__pch.h"
#include "VFsmCacheCtrl__Syms.h"
#include "VFsmCacheCtrl___024root.h"

#ifdef VL_DEBUG
VL_ATTR_COLD void VFsmCacheCtrl___024root___dump_triggers__stl(VFsmCacheCtrl___024root* vlSelf);
#endif  // VL_DEBUG

VL_ATTR_COLD void VFsmCacheCtrl___024root___eval_triggers__stl(VFsmCacheCtrl___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmCacheCtrl___024root___eval_triggers__stl\n"); );
    VFsmCacheCtrl__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    vlSelfRef.__VstlTriggered.set(0U, (IData)(vlSelfRef.__VstlFirstIteration));
#ifdef VL_DEBUG
    if (VL_UNLIKELY(vlSymsp->_vm_contextp__->debug())) {
        VFsmCacheCtrl___024root___dump_triggers__stl(vlSelf);
    }
#endif
}
