// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Design implementation internals
// See VFsmAxi4Fill.h for the primary calling header

#include "VFsmAxi4Fill__pch.h"
#include "VFsmAxi4Fill___024root.h"

void VFsmAxi4Fill___024root___ico_sequent__TOP__0(VFsmAxi4Fill___024root* vlSelf);

void VFsmAxi4Fill___024root___eval_ico(VFsmAxi4Fill___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Fill___024root___eval_ico\n"); );
    VFsmAxi4Fill__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    if ((1ULL & vlSelfRef.__VicoTriggered.word(0U))) {
        VFsmAxi4Fill___024root___ico_sequent__TOP__0(vlSelf);
    }
}

extern const VlUnpacked<CData/*1:0*/, 64> VFsmAxi4Fill__ConstPool__TABLE_h621fdd92_0;

VL_INLINE_OPT void VFsmAxi4Fill___024root___ico_sequent__TOP__0(VFsmAxi4Fill___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Fill___024root___ico_sequent__TOP__0\n"); );
    VFsmAxi4Fill__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Init
    CData/*5:0*/ __Vtableidx1;
    __Vtableidx1 = 0;
    // Body
    __Vtableidx1 = ((((IData)(vlSelfRef.fill_start) 
                      << 5U) | (((IData)(vlSelfRef.ar_ready) 
                                 << 4U) | ((IData)(vlSelfRef.r_last) 
                                           << 3U))) 
                    | (((IData)(vlSelfRef.r_valid) 
                        << 2U) | (IData)(vlSelfRef.FsmAxi4Fill__DOT__state_r)));
    vlSelfRef.FsmAxi4Fill__DOT__state_next = VFsmAxi4Fill__ConstPool__TABLE_h621fdd92_0
        [__Vtableidx1];
}

void VFsmAxi4Fill___024root___eval_triggers__ico(VFsmAxi4Fill___024root* vlSelf);

bool VFsmAxi4Fill___024root___eval_phase__ico(VFsmAxi4Fill___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Fill___024root___eval_phase__ico\n"); );
    VFsmAxi4Fill__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Init
    CData/*0:0*/ __VicoExecute;
    // Body
    VFsmAxi4Fill___024root___eval_triggers__ico(vlSelf);
    __VicoExecute = vlSelfRef.__VicoTriggered.any();
    if (__VicoExecute) {
        VFsmAxi4Fill___024root___eval_ico(vlSelf);
    }
    return (__VicoExecute);
}

void VFsmAxi4Fill___024root___eval_act(VFsmAxi4Fill___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Fill___024root___eval_act\n"); );
    VFsmAxi4Fill__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
}

void VFsmAxi4Fill___024root___nba_sequent__TOP__0(VFsmAxi4Fill___024root* vlSelf);

void VFsmAxi4Fill___024root___eval_nba(VFsmAxi4Fill___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Fill___024root___eval_nba\n"); );
    VFsmAxi4Fill__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    if ((1ULL & vlSelfRef.__VnbaTriggered.word(0U))) {
        VFsmAxi4Fill___024root___nba_sequent__TOP__0(vlSelf);
    }
}

VL_INLINE_OPT void VFsmAxi4Fill___024root___nba_sequent__TOP__0(VFsmAxi4Fill___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Fill___024root___nba_sequent__TOP__0\n"); );
    VFsmAxi4Fill__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Init
    CData/*5:0*/ __Vtableidx1;
    __Vtableidx1 = 0;
    CData/*3:0*/ __Vdly__FsmAxi4Fill__DOT__beat_ctr_r;
    __Vdly__FsmAxi4Fill__DOT__beat_ctr_r = 0;
    // Body
    __Vdly__FsmAxi4Fill__DOT__beat_ctr_r = vlSelfRef.FsmAxi4Fill__DOT__beat_ctr_r;
    if (vlSelfRef.rst) {
        vlSelfRef.FsmAxi4Fill__DOT__fill_addr_r = 0ULL;
        __Vdly__FsmAxi4Fill__DOT__beat_ctr_r = 0U;
        vlSelfRef.FsmAxi4Fill__DOT__state_r = 0U;
    } else {
        if ((0U == (IData)(vlSelfRef.FsmAxi4Fill__DOT__state_r))) {
            if (vlSelfRef.fill_start) {
                vlSelfRef.FsmAxi4Fill__DOT__fill_addr_r 
                    = vlSelfRef.fill_addr;
                __Vdly__FsmAxi4Fill__DOT__beat_ctr_r = 0U;
            }
        } else if ((2U == (IData)(vlSelfRef.FsmAxi4Fill__DOT__state_r))) {
            if (vlSelfRef.r_valid) {
                if ((0U == (IData)(vlSelfRef.FsmAxi4Fill__DOT__beat_ctr_r))) {
                    vlSelfRef.fill_word_0 = vlSelfRef.r_data;
                } else if ((1U == (IData)(vlSelfRef.FsmAxi4Fill__DOT__beat_ctr_r))) {
                    vlSelfRef.fill_word_1 = vlSelfRef.r_data;
                } else if ((2U == (IData)(vlSelfRef.FsmAxi4Fill__DOT__beat_ctr_r))) {
                    vlSelfRef.fill_word_2 = vlSelfRef.r_data;
                } else if ((3U == (IData)(vlSelfRef.FsmAxi4Fill__DOT__beat_ctr_r))) {
                    vlSelfRef.fill_word_3 = vlSelfRef.r_data;
                } else if ((4U == (IData)(vlSelfRef.FsmAxi4Fill__DOT__beat_ctr_r))) {
                    vlSelfRef.fill_word_4 = vlSelfRef.r_data;
                } else if ((5U == (IData)(vlSelfRef.FsmAxi4Fill__DOT__beat_ctr_r))) {
                    vlSelfRef.fill_word_5 = vlSelfRef.r_data;
                } else if ((6U == (IData)(vlSelfRef.FsmAxi4Fill__DOT__beat_ctr_r))) {
                    vlSelfRef.fill_word_6 = vlSelfRef.r_data;
                } else if ((7U == (IData)(vlSelfRef.FsmAxi4Fill__DOT__beat_ctr_r))) {
                    vlSelfRef.fill_word_7 = vlSelfRef.r_data;
                }
                __Vdly__FsmAxi4Fill__DOT__beat_ctr_r 
                    = (0xfU & ((IData)(1U) + (IData)(vlSelfRef.FsmAxi4Fill__DOT__beat_ctr_r)));
            }
        }
        vlSelfRef.FsmAxi4Fill__DOT__state_r = vlSelfRef.FsmAxi4Fill__DOT__state_next;
    }
    vlSelfRef.FsmAxi4Fill__DOT__beat_ctr_r = __Vdly__FsmAxi4Fill__DOT__beat_ctr_r;
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

void VFsmAxi4Fill___024root___eval_triggers__act(VFsmAxi4Fill___024root* vlSelf);

bool VFsmAxi4Fill___024root___eval_phase__act(VFsmAxi4Fill___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Fill___024root___eval_phase__act\n"); );
    VFsmAxi4Fill__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Init
    VlTriggerVec<1> __VpreTriggered;
    CData/*0:0*/ __VactExecute;
    // Body
    VFsmAxi4Fill___024root___eval_triggers__act(vlSelf);
    __VactExecute = vlSelfRef.__VactTriggered.any();
    if (__VactExecute) {
        __VpreTriggered.andNot(vlSelfRef.__VactTriggered, vlSelfRef.__VnbaTriggered);
        vlSelfRef.__VnbaTriggered.thisOr(vlSelfRef.__VactTriggered);
        VFsmAxi4Fill___024root___eval_act(vlSelf);
    }
    return (__VactExecute);
}

bool VFsmAxi4Fill___024root___eval_phase__nba(VFsmAxi4Fill___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Fill___024root___eval_phase__nba\n"); );
    VFsmAxi4Fill__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Init
    CData/*0:0*/ __VnbaExecute;
    // Body
    __VnbaExecute = vlSelfRef.__VnbaTriggered.any();
    if (__VnbaExecute) {
        VFsmAxi4Fill___024root___eval_nba(vlSelf);
        vlSelfRef.__VnbaTriggered.clear();
    }
    return (__VnbaExecute);
}

#ifdef VL_DEBUG
VL_ATTR_COLD void VFsmAxi4Fill___024root___dump_triggers__ico(VFsmAxi4Fill___024root* vlSelf);
#endif  // VL_DEBUG
#ifdef VL_DEBUG
VL_ATTR_COLD void VFsmAxi4Fill___024root___dump_triggers__nba(VFsmAxi4Fill___024root* vlSelf);
#endif  // VL_DEBUG
#ifdef VL_DEBUG
VL_ATTR_COLD void VFsmAxi4Fill___024root___dump_triggers__act(VFsmAxi4Fill___024root* vlSelf);
#endif  // VL_DEBUG

void VFsmAxi4Fill___024root___eval(VFsmAxi4Fill___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Fill___024root___eval\n"); );
    VFsmAxi4Fill__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Init
    IData/*31:0*/ __VicoIterCount;
    CData/*0:0*/ __VicoContinue;
    IData/*31:0*/ __VnbaIterCount;
    CData/*0:0*/ __VnbaContinue;
    // Body
    __VicoIterCount = 0U;
    vlSelfRef.__VicoFirstIteration = 1U;
    __VicoContinue = 1U;
    while (__VicoContinue) {
        if (VL_UNLIKELY(((0x64U < __VicoIterCount)))) {
#ifdef VL_DEBUG
            VFsmAxi4Fill___024root___dump_triggers__ico(vlSelf);
#endif
            VL_FATAL_MT("tests/l1d/FsmAxi4Fill.sv", 4, "", "Input combinational region did not converge.");
        }
        __VicoIterCount = ((IData)(1U) + __VicoIterCount);
        __VicoContinue = 0U;
        if (VFsmAxi4Fill___024root___eval_phase__ico(vlSelf)) {
            __VicoContinue = 1U;
        }
        vlSelfRef.__VicoFirstIteration = 0U;
    }
    __VnbaIterCount = 0U;
    __VnbaContinue = 1U;
    while (__VnbaContinue) {
        if (VL_UNLIKELY(((0x64U < __VnbaIterCount)))) {
#ifdef VL_DEBUG
            VFsmAxi4Fill___024root___dump_triggers__nba(vlSelf);
#endif
            VL_FATAL_MT("tests/l1d/FsmAxi4Fill.sv", 4, "", "NBA region did not converge.");
        }
        __VnbaIterCount = ((IData)(1U) + __VnbaIterCount);
        __VnbaContinue = 0U;
        vlSelfRef.__VactIterCount = 0U;
        vlSelfRef.__VactContinue = 1U;
        while (vlSelfRef.__VactContinue) {
            if (VL_UNLIKELY(((0x64U < vlSelfRef.__VactIterCount)))) {
#ifdef VL_DEBUG
                VFsmAxi4Fill___024root___dump_triggers__act(vlSelf);
#endif
                VL_FATAL_MT("tests/l1d/FsmAxi4Fill.sv", 4, "", "Active region did not converge.");
            }
            vlSelfRef.__VactIterCount = ((IData)(1U) 
                                         + vlSelfRef.__VactIterCount);
            vlSelfRef.__VactContinue = 0U;
            if (VFsmAxi4Fill___024root___eval_phase__act(vlSelf)) {
                vlSelfRef.__VactContinue = 1U;
            }
        }
        if (VFsmAxi4Fill___024root___eval_phase__nba(vlSelf)) {
            __VnbaContinue = 1U;
        }
    }
}

#ifdef VL_DEBUG
void VFsmAxi4Fill___024root___eval_debug_assertions(VFsmAxi4Fill___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmAxi4Fill___024root___eval_debug_assertions\n"); );
    VFsmAxi4Fill__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    if (VL_UNLIKELY(((vlSelfRef.clk & 0xfeU)))) {
        Verilated::overWidthError("clk");}
    if (VL_UNLIKELY(((vlSelfRef.rst & 0xfeU)))) {
        Verilated::overWidthError("rst");}
    if (VL_UNLIKELY(((vlSelfRef.fill_start & 0xfeU)))) {
        Verilated::overWidthError("fill_start");}
    if (VL_UNLIKELY(((vlSelfRef.ar_ready & 0xfeU)))) {
        Verilated::overWidthError("ar_ready");}
    if (VL_UNLIKELY(((vlSelfRef.r_valid & 0xfeU)))) {
        Verilated::overWidthError("r_valid");}
    if (VL_UNLIKELY(((vlSelfRef.r_id & 0xf0U)))) {
        Verilated::overWidthError("r_id");}
    if (VL_UNLIKELY(((vlSelfRef.r_resp & 0xfcU)))) {
        Verilated::overWidthError("r_resp");}
    if (VL_UNLIKELY(((vlSelfRef.r_last & 0xfeU)))) {
        Verilated::overWidthError("r_last");}
}
#endif  // VL_DEBUG
