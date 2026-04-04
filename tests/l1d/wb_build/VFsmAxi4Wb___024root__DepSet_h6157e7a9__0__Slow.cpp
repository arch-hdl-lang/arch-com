// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Design implementation internals
// See VFsmAxi4Wb.h for the primary calling header

#include "VFsmAxi4Wb__pch.h"
#include "VFsmAxi4Wb___024root.h"

VL_ATTR_COLD void VFsmAxi4Wb___024root___eval_static(VFsmAxi4Wb___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Wb___024root___eval_static\n"); );
    VFsmAxi4Wb__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
}

VL_ATTR_COLD void VFsmAxi4Wb___024root___eval_initial(VFsmAxi4Wb___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Wb___024root___eval_initial\n"); );
    VFsmAxi4Wb__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    vlSelfRef.__Vtrigprevexpr___TOP__clk__0 = vlSelfRef.clk;
}

VL_ATTR_COLD void VFsmAxi4Wb___024root___eval_final(VFsmAxi4Wb___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Wb___024root___eval_final\n"); );
    VFsmAxi4Wb__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
}

#ifdef VL_DEBUG
VL_ATTR_COLD void VFsmAxi4Wb___024root___dump_triggers__stl(VFsmAxi4Wb___024root* vlSelf);
#endif  // VL_DEBUG
VL_ATTR_COLD bool VFsmAxi4Wb___024root___eval_phase__stl(VFsmAxi4Wb___024root* vlSelf);

VL_ATTR_COLD void VFsmAxi4Wb___024root___eval_settle(VFsmAxi4Wb___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Wb___024root___eval_settle\n"); );
    VFsmAxi4Wb__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
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
            VFsmAxi4Wb___024root___dump_triggers__stl(vlSelf);
#endif
            VL_FATAL_MT("tests/l1d/FsmAxi4Wb.sv", 4, "", "Settle region did not converge.");
        }
        __VstlIterCount = ((IData)(1U) + __VstlIterCount);
        __VstlContinue = 0U;
        if (VFsmAxi4Wb___024root___eval_phase__stl(vlSelf)) {
            __VstlContinue = 1U;
        }
        vlSelfRef.__VstlFirstIteration = 0U;
    }
}

#ifdef VL_DEBUG
VL_ATTR_COLD void VFsmAxi4Wb___024root___dump_triggers__stl(VFsmAxi4Wb___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Wb___024root___dump_triggers__stl\n"); );
    VFsmAxi4Wb__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
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

VL_ATTR_COLD void VFsmAxi4Wb___024root___stl_sequent__TOP__0(VFsmAxi4Wb___024root* vlSelf);

VL_ATTR_COLD void VFsmAxi4Wb___024root___eval_stl(VFsmAxi4Wb___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Wb___024root___eval_stl\n"); );
    VFsmAxi4Wb__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    if ((1ULL & vlSelfRef.__VstlTriggered.word(0U))) {
        VFsmAxi4Wb___024root___stl_sequent__TOP__0(vlSelf);
    }
}

extern const VlUnpacked<CData/*1:0*/, 1024> VFsmAxi4Wb__ConstPool__TABLE_h49293a7b_0;

VL_ATTR_COLD void VFsmAxi4Wb___024root___stl_sequent__TOP__0(VFsmAxi4Wb___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Wb___024root___stl_sequent__TOP__0\n"); );
    VFsmAxi4Wb__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Init
    SData/*9:0*/ __Vtableidx1;
    __Vtableidx1 = 0;
    // Body
    vlSelfRef.aw_valid = 0U;
    vlSelfRef.aw_id = 0U;
    vlSelfRef.aw_len = 0U;
    vlSelfRef.aw_size = 0U;
    vlSelfRef.aw_burst = 0U;
    vlSelfRef.w_valid = 0U;
    vlSelfRef.w_strb = 0U;
    vlSelfRef.b_ready = 0U;
    vlSelfRef.wb_done = 0U;
    vlSelfRef.aw_addr = 0ULL;
    if ((1U & (~ ((IData)(vlSelfRef.FsmAxi4Wb__DOT__state_r) 
                  >> 1U)))) {
        if ((1U & (IData)(vlSelfRef.FsmAxi4Wb__DOT__state_r))) {
            vlSelfRef.aw_valid = 1U;
            vlSelfRef.aw_id = 1U;
            vlSelfRef.aw_len = 7U;
            vlSelfRef.aw_size = 3U;
            vlSelfRef.aw_burst = 1U;
            vlSelfRef.aw_addr = (0xffffffffffffffc0ULL 
                                 & vlSelfRef.FsmAxi4Wb__DOT__wb_addr_r);
        }
    }
    vlSelfRef.w_last = 0U;
    __Vtableidx1 = ((((IData)(vlSelfRef.wb_start) << 9U) 
                     | ((IData)(vlSelfRef.aw_ready) 
                        << 8U)) | (((IData)(vlSelfRef.FsmAxi4Wb__DOT__beat_ctr_r) 
                                    << 4U) | (((IData)(vlSelfRef.w_ready) 
                                               << 3U) 
                                              | (((IData)(vlSelfRef.b_valid) 
                                                  << 2U) 
                                                 | (IData)(vlSelfRef.FsmAxi4Wb__DOT__state_r)))));
    vlSelfRef.FsmAxi4Wb__DOT__state_next = VFsmAxi4Wb__ConstPool__TABLE_h49293a7b_0
        [__Vtableidx1];
    vlSelfRef.w_data = 0ULL;
    if ((2U & (IData)(vlSelfRef.FsmAxi4Wb__DOT__state_r))) {
        if ((1U & (~ (IData)(vlSelfRef.FsmAxi4Wb__DOT__state_r)))) {
            vlSelfRef.w_valid = 1U;
            vlSelfRef.w_strb = 0xffU;
            vlSelfRef.w_last = (7U == (IData)(vlSelfRef.FsmAxi4Wb__DOT__beat_ctr_r));
            vlSelfRef.w_data = 0ULL;
            if ((0U == (IData)(vlSelfRef.FsmAxi4Wb__DOT__beat_ctr_r))) {
                vlSelfRef.w_data = vlSelfRef.wb_word_0;
            } else if ((1U == (IData)(vlSelfRef.FsmAxi4Wb__DOT__beat_ctr_r))) {
                vlSelfRef.w_data = vlSelfRef.wb_word_1;
            } else if ((2U == (IData)(vlSelfRef.FsmAxi4Wb__DOT__beat_ctr_r))) {
                vlSelfRef.w_data = vlSelfRef.wb_word_2;
            } else if ((3U == (IData)(vlSelfRef.FsmAxi4Wb__DOT__beat_ctr_r))) {
                vlSelfRef.w_data = vlSelfRef.wb_word_3;
            } else if ((4U == (IData)(vlSelfRef.FsmAxi4Wb__DOT__beat_ctr_r))) {
                vlSelfRef.w_data = vlSelfRef.wb_word_4;
            } else if ((5U == (IData)(vlSelfRef.FsmAxi4Wb__DOT__beat_ctr_r))) {
                vlSelfRef.w_data = vlSelfRef.wb_word_5;
            } else if ((6U == (IData)(vlSelfRef.FsmAxi4Wb__DOT__beat_ctr_r))) {
                vlSelfRef.w_data = vlSelfRef.wb_word_6;
            } else if ((7U == (IData)(vlSelfRef.FsmAxi4Wb__DOT__beat_ctr_r))) {
                vlSelfRef.w_data = vlSelfRef.wb_word_7;
            }
        }
        if ((1U & (IData)(vlSelfRef.FsmAxi4Wb__DOT__state_r))) {
            vlSelfRef.b_ready = 1U;
            vlSelfRef.wb_done = vlSelfRef.b_valid;
        }
    }
}

VL_ATTR_COLD void VFsmAxi4Wb___024root___eval_triggers__stl(VFsmAxi4Wb___024root* vlSelf);

VL_ATTR_COLD bool VFsmAxi4Wb___024root___eval_phase__stl(VFsmAxi4Wb___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Wb___024root___eval_phase__stl\n"); );
    VFsmAxi4Wb__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Init
    CData/*0:0*/ __VstlExecute;
    // Body
    VFsmAxi4Wb___024root___eval_triggers__stl(vlSelf);
    __VstlExecute = vlSelfRef.__VstlTriggered.any();
    if (__VstlExecute) {
        VFsmAxi4Wb___024root___eval_stl(vlSelf);
    }
    return (__VstlExecute);
}

#ifdef VL_DEBUG
VL_ATTR_COLD void VFsmAxi4Wb___024root___dump_triggers__ico(VFsmAxi4Wb___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Wb___024root___dump_triggers__ico\n"); );
    VFsmAxi4Wb__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
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
VL_ATTR_COLD void VFsmAxi4Wb___024root___dump_triggers__act(VFsmAxi4Wb___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Wb___024root___dump_triggers__act\n"); );
    VFsmAxi4Wb__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
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
VL_ATTR_COLD void VFsmAxi4Wb___024root___dump_triggers__nba(VFsmAxi4Wb___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Wb___024root___dump_triggers__nba\n"); );
    VFsmAxi4Wb__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
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

VL_ATTR_COLD void VFsmAxi4Wb___024root___ctor_var_reset(VFsmAxi4Wb___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Wb___024root___ctor_var_reset\n"); );
    VFsmAxi4Wb__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    vlSelf->clk = VL_RAND_RESET_I(1);
    vlSelf->rst = VL_RAND_RESET_I(1);
    vlSelf->wb_start = VL_RAND_RESET_I(1);
    vlSelf->wb_addr = VL_RAND_RESET_Q(64);
    vlSelf->wb_done = VL_RAND_RESET_I(1);
    vlSelf->wb_word_0 = VL_RAND_RESET_Q(64);
    vlSelf->wb_word_1 = VL_RAND_RESET_Q(64);
    vlSelf->wb_word_2 = VL_RAND_RESET_Q(64);
    vlSelf->wb_word_3 = VL_RAND_RESET_Q(64);
    vlSelf->wb_word_4 = VL_RAND_RESET_Q(64);
    vlSelf->wb_word_5 = VL_RAND_RESET_Q(64);
    vlSelf->wb_word_6 = VL_RAND_RESET_Q(64);
    vlSelf->wb_word_7 = VL_RAND_RESET_Q(64);
    vlSelf->aw_valid = VL_RAND_RESET_I(1);
    vlSelf->aw_ready = VL_RAND_RESET_I(1);
    vlSelf->aw_addr = VL_RAND_RESET_Q(64);
    vlSelf->aw_id = VL_RAND_RESET_I(4);
    vlSelf->aw_len = VL_RAND_RESET_I(8);
    vlSelf->aw_size = VL_RAND_RESET_I(3);
    vlSelf->aw_burst = VL_RAND_RESET_I(2);
    vlSelf->w_valid = VL_RAND_RESET_I(1);
    vlSelf->w_ready = VL_RAND_RESET_I(1);
    vlSelf->w_data = VL_RAND_RESET_Q(64);
    vlSelf->w_strb = VL_RAND_RESET_I(8);
    vlSelf->w_last = VL_RAND_RESET_I(1);
    vlSelf->b_valid = VL_RAND_RESET_I(1);
    vlSelf->b_ready = VL_RAND_RESET_I(1);
    vlSelf->b_id = VL_RAND_RESET_I(4);
    vlSelf->b_resp = VL_RAND_RESET_I(2);
    vlSelf->FsmAxi4Wb__DOT__state_r = VL_RAND_RESET_I(2);
    vlSelf->FsmAxi4Wb__DOT__state_next = VL_RAND_RESET_I(2);
    vlSelf->FsmAxi4Wb__DOT__wb_addr_r = VL_RAND_RESET_Q(64);
    vlSelf->FsmAxi4Wb__DOT__beat_ctr_r = VL_RAND_RESET_I(4);
    vlSelf->__Vtrigprevexpr___TOP__clk__0 = VL_RAND_RESET_I(1);
}
