// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Design implementation internals
// See VAesCipherTop.h for the primary calling header

#include "VAesCipherTop__pch.h"
#include "VAesCipherTop__Syms.h"
#include "VAesCipherTop___024root.h"

#ifdef VL_DEBUG
VL_ATTR_COLD void VAesCipherTop___024root___dump_triggers__stl(VAesCipherTop___024root* vlSelf);
#endif  // VL_DEBUG

VL_ATTR_COLD void VAesCipherTop___024root___eval_triggers__stl(VAesCipherTop___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VAesCipherTop___024root___eval_triggers__stl\n"); );
    VAesCipherTop__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    vlSelfRef.__VstlTriggered.set(0U, (IData)(vlSelfRef.__VstlFirstIteration));
#ifdef VL_DEBUG
    if (VL_UNLIKELY(vlSymsp->_vm_contextp__->debug())) {
        VAesCipherTop___024root___dump_triggers__stl(vlSelf);
    }
#endif
}
