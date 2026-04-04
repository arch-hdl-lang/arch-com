// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Design implementation internals
// See VL1DCache.h for the primary calling header

#include "VL1DCache__pch.h"
#include "VL1DCache__Syms.h"
#include "VL1DCache___024root.h"

#ifdef VL_DEBUG
VL_ATTR_COLD void VL1DCache___024root___dump_triggers__stl(VL1DCache___024root* vlSelf);
#endif  // VL_DEBUG

VL_ATTR_COLD void VL1DCache___024root___eval_triggers__stl(VL1DCache___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VL1DCache___024root___eval_triggers__stl\n"); );
    VL1DCache__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    vlSelfRef.__VstlTriggered.set(0U, (IData)(vlSelfRef.__VstlFirstIteration));
#ifdef VL_DEBUG
    if (VL_UNLIKELY(vlSymsp->_vm_contextp__->debug())) {
        VL1DCache___024root___dump_triggers__stl(vlSelf);
    }
#endif
}
