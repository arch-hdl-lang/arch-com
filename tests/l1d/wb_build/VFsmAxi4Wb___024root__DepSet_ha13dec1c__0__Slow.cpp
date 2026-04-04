// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Design implementation internals
// See VFsmAxi4Wb.h for the primary calling header

#include "VFsmAxi4Wb__pch.h"
#include "VFsmAxi4Wb__Syms.h"
#include "VFsmAxi4Wb___024root.h"

#ifdef VL_DEBUG
VL_ATTR_COLD void VFsmAxi4Wb___024root___dump_triggers__stl(VFsmAxi4Wb___024root* vlSelf);
#endif  // VL_DEBUG

VL_ATTR_COLD void VFsmAxi4Wb___024root___eval_triggers__stl(VFsmAxi4Wb___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Wb___024root___eval_triggers__stl\n"); );
    VFsmAxi4Wb__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    vlSelfRef.__VstlTriggered.set(0U, (IData)(vlSelfRef.__VstlFirstIteration));
#ifdef VL_DEBUG
    if (VL_UNLIKELY(vlSymsp->_vm_contextp__->debug())) {
        VFsmAxi4Wb___024root___dump_triggers__stl(vlSelf);
    }
#endif
}
