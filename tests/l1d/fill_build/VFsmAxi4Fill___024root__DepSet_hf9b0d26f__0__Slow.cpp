// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Design implementation internals
// See VFsmAxi4Fill.h for the primary calling header

#include "VFsmAxi4Fill__pch.h"
#include "VFsmAxi4Fill___024root.h"

VL_ATTR_COLD void VFsmAxi4Fill___024root___eval_static(VFsmAxi4Fill___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Fill___024root___eval_static\n"); );
    VFsmAxi4Fill__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
}

VL_ATTR_COLD void VFsmAxi4Fill___024root___eval_initial(VFsmAxi4Fill___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Fill___024root___eval_initial\n"); );
    VFsmAxi4Fill__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    vlSelfRef.__Vtrigprevexpr___TOP__clk__0 = vlSelfRef.clk;
}

VL_ATTR_COLD void VFsmAxi4Fill___024root___eval_final(VFsmAxi4Fill___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Fill___024root___eval_final\n"); );
    VFsmAxi4Fill__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
}

#ifdef VL_DEBUG
VL_ATTR_COLD void VFsmAxi4Fill___024root___dump_triggers__stl(VFsmAxi4Fill___024root* vlSelf);
#endif  // VL_DEBUG
VL_ATTR_COLD bool VFsmAxi4Fill___024root___eval_phase__stl(VFsmAxi4Fill___024root* vlSelf);

VL_ATTR_COLD void VFsmAxi4Fill___024root___eval_settle(VFsmAxi4Fill___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Fill___024root___eval_settle\n"); );
    VFsmAxi4Fill__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Init
    IData/*31:0*/ __VstlIterCount;
    CData/*0:0*/ __VstlContinue;
    // Body
    __VstlIterCount = 0U;
    vlSelfRef.__VstlFirstIteration = 1U;
    __VstlContinue = 1U;
    while (__VstlContinue) {
        if (VL_UNLIKELY(((0x64U < __VstlIterCount)))) {
#ifdef VL_DEBUG
            VFsmAxi4Fill___024root___dump_triggers__stl(vlSelf);
#endif
            VL_FATAL_MT("tests/l1d/FsmAxi4Fill.sv", 4, "", "Settle region did not converge.");
        }
        __VstlIterCount = ((IData)(1U) + __VstlIterCount);
        __VstlContinue = 0U;
        if (VFsmAxi4Fill___024root___eval_phase__stl(vlSelf)) {
            __VstlContinue = 1U;
        }
        vlSelfRef.__VstlFirstIteration = 0U;
    }
}

#ifdef VL_DEBUG
VL_ATTR_COLD void VFsmAxi4Fill___024root___dump_triggers__stl(VFsmAxi4Fill___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Fill___024root___dump_triggers__stl\n"); );
    VFsmAxi4Fill__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    if ((1U & (~ vlSelfRef.__VstlTriggered.any()))) {
        VL_DBG_MSGF("         No triggers active\n");
    }
    if ((1ULL & vlSelfRef.__VstlTriggered.word(0U))) {
        VL_DBG_MSGF("         'stl' region trigger index 0 is active: Internal 'stl' trigger - first iteration\n");
    }
}
#endif  // VL_DEBUG

VL_ATTR_COLD void VFsmAxi4Fill___024root___stl_sequent__TOP__0(VFsmAxi4Fill___024root* vlSelf);

VL_ATTR_COLD void VFsmAxi4Fill___024root___eval_stl(VFsmAxi4Fill___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Fill___024root___eval_stl\n"); );
    VFsmAxi4Fill__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    if ((1ULL & vlSelfRef.__VstlTriggered.word(0U))) {
        VFsmAxi4Fill___024root___stl_sequent__TOP__0(vlSelf);
    }
}

extern const VlUnpacked<CData/*1:0*/, 64> VFsmAxi4Fill__ConstPool__TABLE_h621fdd92_0;

VL_ATTR_COLD void VFsmAxi4Fill___024root___stl_sequent__TOP__0(VFsmAxi4Fill___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Fill___024root___stl_sequent__TOP__0\n"); );
    VFsmAxi4Fill__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Init
    CData/*5:0*/ __Vtableidx1;
    __Vtableidx1 = 0;
    // Body
    vlSelfRef.fill_done = 0U;
    vlSelfRef.ar_valid = 0U;
    vlSelfRef.ar_id = 0U;
    vlSelfRef.ar_len = 0U;
    vlSelfRef.ar_size = 0U;
    vlSelfRef.ar_burst = 0U;
    vlSelfRef.r_ready = 0U;
    if ((2U & (IData)(vlSelfRef.FsmAxi4Fill__DOT__state_r))) {
        if ((1U & (IData)(vlSelfRef.FsmAxi4Fill__DOT__state_r))) {
            vlSelfRef.fill_done = 1U;
        }
        if ((1U & (~ (IData)(vlSelfRef.FsmAxi4Fill__DOT__state_r)))) {
            vlSelfRef.r_ready = 1U;
        }
    }
    vlSelfRef.ar_addr = 0ULL;
    if ((1U & (~ ((IData)(vlSelfRef.FsmAxi4Fill__DOT__state_r) 
                  >> 1U)))) {
        if ((1U & (IData)(vlSelfRef.FsmAxi4Fill__DOT__state_r))) {
            vlSelfRef.ar_valid = 1U;
            vlSelfRef.ar_id = 0U;
            vlSelfRef.ar_len = 7U;
            vlSelfRef.ar_size = 3U;
            vlSelfRef.ar_burst = 1U;
            vlSelfRef.ar_addr = (0xffffffffffffffc0ULL 
                                 & vlSelfRef.FsmAxi4Fill__DOT__fill_addr_r);
        }
    }
    __Vtableidx1 = ((((IData)(vlSelfRef.fill_start) 
                      << 5U) | (((IData)(vlSelfRef.ar_ready) 
                                 << 4U) | ((IData)(vlSelfRef.r_last) 
                                           << 3U))) 
                    | (((IData)(vlSelfRef.r_valid) 
                        << 2U) | (IData)(vlSelfRef.FsmAxi4Fill__DOT__state_r)));
    vlSelfRef.FsmAxi4Fill__DOT__state_next = VFsmAxi4Fill__ConstPool__TABLE_h621fdd92_0
        [__Vtableidx1];
}

VL_ATTR_COLD void VFsmAxi4Fill___024root___eval_triggers__stl(VFsmAxi4Fill___024root* vlSelf);

VL_ATTR_COLD bool VFsmAxi4Fill___024root___eval_phase__stl(VFsmAxi4Fill___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Fill___024root___eval_phase__stl\n"); );
    VFsmAxi4Fill__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Init
    CData/*0:0*/ __VstlExecute;
    // Body
    VFsmAxi4Fill___024root___eval_triggers__stl(vlSelf);
    __VstlExecute = vlSelfRef.__VstlTriggered.any();
    if (__VstlExecute) {
        VFsmAxi4Fill___024root___eval_stl(vlSelf);
    }
    return (__VstlExecute);
}

#ifdef VL_DEBUG
VL_ATTR_COLD void VFsmAxi4Fill___024root___dump_triggers__ico(VFsmAxi4Fill___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Fill___024root___dump_triggers__ico\n"); );
    VFsmAxi4Fill__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    if ((1U & (~ vlSelfRef.__VicoTriggered.any()))) {
        VL_DBG_MSGF("         No triggers active\n");
    }
    if ((1ULL & vlSelfRef.__VicoTriggered.word(0U))) {
        VL_DBG_MSGF("         'ico' region trigger index 0 is active: Internal 'ico' trigger - first iteration\n");
    }
}
#endif  // VL_DEBUG

#ifdef VL_DEBUG
VL_ATTR_COLD void VFsmAxi4Fill___024root___dump_triggers__act(VFsmAxi4Fill___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Fill___024root___dump_triggers__act\n"); );
    VFsmAxi4Fill__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    if ((1U & (~ vlSelfRef.__VactTriggered.any()))) {
        VL_DBG_MSGF("         No triggers active\n");
    }
    if ((1ULL & vlSelfRef.__VactTriggered.word(0U))) {
        VL_DBG_MSGF("         'act' region trigger index 0 is active: @(posedge clk)\n");
    }
}
#endif  // VL_DEBUG

#ifdef VL_DEBUG
VL_ATTR_COLD void VFsmAxi4Fill___024root___dump_triggers__nba(VFsmAxi4Fill___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Fill___024root___dump_triggers__nba\n"); );
    VFsmAxi4Fill__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    if ((1U & (~ vlSelfRef.__VnbaTriggered.any()))) {
        VL_DBG_MSGF("         No triggers active\n");
    }
    if ((1ULL & vlSelfRef.__VnbaTriggered.word(0U))) {
        VL_DBG_MSGF("         'nba' region trigger index 0 is active: @(posedge clk)\n");
    }
}
#endif  // VL_DEBUG

VL_ATTR_COLD void VFsmAxi4Fill___024root___ctor_var_reset(VFsmAxi4Fill___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Fill___024root___ctor_var_reset\n"); );
    VFsmAxi4Fill__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    vlSelf->clk = VL_RAND_RESET_I(1);
    vlSelf->rst = VL_RAND_RESET_I(1);
    vlSelf->fill_start = VL_RAND_RESET_I(1);
    vlSelf->fill_addr = VL_RAND_RESET_Q(64);
    vlSelf->fill_done = VL_RAND_RESET_I(1);
    vlSelf->fill_word_0 = VL_RAND_RESET_Q(64);
    vlSelf->fill_word_1 = VL_RAND_RESET_Q(64);
    vlSelf->fill_word_2 = VL_RAND_RESET_Q(64);
    vlSelf->fill_word_3 = VL_RAND_RESET_Q(64);
    vlSelf->fill_word_4 = VL_RAND_RESET_Q(64);
    vlSelf->fill_word_5 = VL_RAND_RESET_Q(64);
    vlSelf->fill_word_6 = VL_RAND_RESET_Q(64);
    vlSelf->fill_word_7 = VL_RAND_RESET_Q(64);
    vlSelf->ar_valid = VL_RAND_RESET_I(1);
    vlSelf->ar_ready = VL_RAND_RESET_I(1);
    vlSelf->ar_addr = VL_RAND_RESET_Q(64);
    vlSelf->ar_id = VL_RAND_RESET_I(4);
    vlSelf->ar_len = VL_RAND_RESET_I(8);
    vlSelf->ar_size = VL_RAND_RESET_I(3);
    vlSelf->ar_burst = VL_RAND_RESET_I(2);
    vlSelf->r_valid = VL_RAND_RESET_I(1);
    vlSelf->r_ready = VL_RAND_RESET_I(1);
    vlSelf->r_data = VL_RAND_RESET_Q(64);
    vlSelf->r_id = VL_RAND_RESET_I(4);
    vlSelf->r_resp = VL_RAND_RESET_I(2);
    vlSelf->r_last = VL_RAND_RESET_I(1);
    vlSelf->FsmAxi4Fill__DOT__state_r = VL_RAND_RESET_I(2);
    vlSelf->FsmAxi4Fill__DOT__state_next = VL_RAND_RESET_I(2);
    vlSelf->FsmAxi4Fill__DOT__fill_addr_r = VL_RAND_RESET_Q(64);
    vlSelf->FsmAxi4Fill__DOT__beat_ctr_r = VL_RAND_RESET_I(4);
    vlSelf->__Vtrigprevexpr___TOP__clk__0 = VL_RAND_RESET_I(1);
}
