// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Design implementation internals
// See VL1DCache.h for the primary calling header

#include "VL1DCache__pch.h"
#include "VL1DCache___024root.h"

void VL1DCache___024root___ico_sequent__TOP__0(VL1DCache___024root* vlSelf);

void VL1DCache___024root___eval_ico(VL1DCache___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VL1DCache___024root___eval_ico\n"); );
    VL1DCache__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    if ((1ULL & vlSelfRef.__VicoTriggered.word(0U))) {
        VL1DCache___024root___ico_sequent__TOP__0(vlSelf);
    }
}

extern const VlUnpacked<CData/*1:0*/, 64> VL1DCache__ConstPool__TABLE_h621fdd92_0;
extern const VlUnpacked<CData/*1:0*/, 1024> VL1DCache__ConstPool__TABLE_h49293a7b_0;

VL_INLINE_OPT void VL1DCache___024root___ico_sequent__TOP__0(VL1DCache___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VL1DCache___024root___ico_sequent__TOP__0\n"); );
    VL1DCache__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Init
    CData/*0:0*/ L1DCache__DOT__wb_done_w;
    L1DCache__DOT__wb_done_w = 0;
    CData/*5:0*/ __Vtableidx1;
    __Vtableidx1 = 0;
    SData/*9:0*/ __Vtableidx2;
    __Vtableidx2 = 0;
    // Body
    vlSelfRef.L1DCache__DOT__tag_rd_en_0 = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_en_1 = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_en_2 = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_en_3 = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_en_4 = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_en_5 = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_en_6 = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_en_7 = 0U;
    vlSelfRef.L1DCache__DOT__lru_rd_en_w = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_addr_0 = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_addr_1 = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_addr_2 = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_addr_3 = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_addr_4 = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_addr_5 = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_addr_6 = 0U;
    vlSelfRef.L1DCache__DOT__lru_rd_addr_w = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_addr_7 = 0U;
    if ((1U & (~ ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r) 
                  >> 3U)))) {
        if ((1U & (~ ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r) 
                      >> 2U)))) {
            if ((1U & (~ ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r) 
                          >> 1U)))) {
                if ((1U & (~ (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r)))) {
                    if (vlSelfRef.req_valid) {
                        vlSelfRef.L1DCache__DOT__tag_rd_en_0 = 1U;
                        vlSelfRef.L1DCache__DOT__tag_rd_en_1 = 1U;
                        vlSelfRef.L1DCache__DOT__tag_rd_en_2 = 1U;
                        vlSelfRef.L1DCache__DOT__tag_rd_en_3 = 1U;
                        vlSelfRef.L1DCache__DOT__tag_rd_en_4 = 1U;
                        vlSelfRef.L1DCache__DOT__tag_rd_en_5 = 1U;
                        vlSelfRef.L1DCache__DOT__tag_rd_en_6 = 1U;
                        vlSelfRef.L1DCache__DOT__tag_rd_en_7 = 1U;
                        vlSelfRef.L1DCache__DOT__lru_rd_en_w = 1U;
                        vlSelfRef.L1DCache__DOT__tag_rd_addr_0 
                            = (0x3fU & (IData)((vlSelfRef.req_vaddr 
                                                >> 6U)));
                        vlSelfRef.L1DCache__DOT__tag_rd_addr_1 
                            = (0x3fU & (IData)((vlSelfRef.req_vaddr 
                                                >> 6U)));
                        vlSelfRef.L1DCache__DOT__tag_rd_addr_2 
                            = (0x3fU & (IData)((vlSelfRef.req_vaddr 
                                                >> 6U)));
                        vlSelfRef.L1DCache__DOT__tag_rd_addr_3 
                            = (0x3fU & (IData)((vlSelfRef.req_vaddr 
                                                >> 6U)));
                        vlSelfRef.L1DCache__DOT__tag_rd_addr_4 
                            = (0x3fU & (IData)((vlSelfRef.req_vaddr 
                                                >> 6U)));
                        vlSelfRef.L1DCache__DOT__tag_rd_addr_5 
                            = (0x3fU & (IData)((vlSelfRef.req_vaddr 
                                                >> 6U)));
                        vlSelfRef.L1DCache__DOT__tag_rd_addr_6 
                            = (0x3fU & (IData)((vlSelfRef.req_vaddr 
                                                >> 6U)));
                        vlSelfRef.L1DCache__DOT__lru_rd_addr_w 
                            = (0x3fU & (IData)((vlSelfRef.req_vaddr 
                                                >> 6U)));
                        vlSelfRef.L1DCache__DOT__tag_rd_addr_7 
                            = (0x3fU & (IData)((vlSelfRef.req_vaddr 
                                                >> 6U)));
                    }
                }
            }
        }
    }
    __Vtableidx1 = ((((IData)(vlSelfRef.L1DCache__DOT__fill_start_w) 
                      << 5U) | (((IData)(vlSelfRef.ar_ready) 
                                 << 4U) | ((IData)(vlSelfRef.r_last) 
                                           << 3U))) 
                    | (((IData)(vlSelfRef.r_valid) 
                        << 2U) | (IData)(vlSelfRef.L1DCache__DOT__fill_fsm__DOT__state_r)));
    vlSelfRef.L1DCache__DOT__fill_fsm__DOT__state_next 
        = VL1DCache__ConstPool__TABLE_h621fdd92_0[__Vtableidx1];
    __Vtableidx2 = ((((IData)(vlSelfRef.L1DCache__DOT__wb_start_w) 
                      << 9U) | ((IData)(vlSelfRef.aw_ready) 
                                << 8U)) | (((IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__beat_ctr_r) 
                                            << 4U) 
                                           | (((IData)(vlSelfRef.w_ready) 
                                               << 3U) 
                                              | (((IData)(vlSelfRef.b_valid) 
                                                  << 2U) 
                                                 | (IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__state_r)))));
    vlSelfRef.L1DCache__DOT__wb_fsm__DOT__state_next 
        = VL1DCache__ConstPool__TABLE_h49293a7b_0[__Vtableidx2];
    L1DCache__DOT__wb_done_w = 0U;
    if ((2U & (IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__state_r))) {
        if ((1U & (IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__state_r))) {
            L1DCache__DOT__wb_done_w = vlSelfRef.b_valid;
        }
    }
    vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next 
        = vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r;
    if ((8U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
        if ((4U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
            vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next 
                = vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r;
        } else if ((2U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
            vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next 
                = vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r;
        } else if ((1U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
            vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next 
                = vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r;
        } else if (L1DCache__DOT__wb_done_w) {
            vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next = 3U;
        }
    } else if ((4U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
        if ((2U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
            if ((1U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
                if ((8U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                    vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next = 8U;
                }
            } else {
                vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next = 0U;
            }
        } else if ((1U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
            if (((7U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r)) 
                 & (~ (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__miss_is_store_r)))) {
                vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next = 0U;
            } else if (((7U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r)) 
                        & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__miss_is_store_r))) {
                vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next = 6U;
            }
        } else if (vlSelfRef.L1DCache__DOT__fill_done_w) {
            vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next = 5U;
        }
    } else if ((2U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
        if ((1U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
            vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next = 4U;
        } else if (vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_hit_r) {
            vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next = 0U;
        } else if (((~ (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_hit_r)) 
                    & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_victim_dirty_r))) {
            vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next = 7U;
        } else if ((1U & ((~ (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_hit_r)) 
                          & (~ (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_victim_dirty_r))))) {
            vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next = 3U;
        }
    } else if ((1U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
        vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next = 2U;
    } else if (vlSelfRef.req_valid) {
        vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next = 1U;
    }
}

void VL1DCache___024root___eval_triggers__ico(VL1DCache___024root* vlSelf);

bool VL1DCache___024root___eval_phase__ico(VL1DCache___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VL1DCache___024root___eval_phase__ico\n"); );
    VL1DCache__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Init
    CData/*0:0*/ __VicoExecute;
    // Body
    VL1DCache___024root___eval_triggers__ico(vlSelf);
    __VicoExecute = vlSelfRef.__VicoTriggered.any();
    if (__VicoExecute) {
        VL1DCache___024root___eval_ico(vlSelf);
    }
    return (__VicoExecute);
}

void VL1DCache___024root___eval_act(VL1DCache___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VL1DCache___024root___eval_act\n"); );
    VL1DCache__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
}

void VL1DCache___024root___nba_sequent__TOP__0(VL1DCache___024root* vlSelf);

void VL1DCache___024root___eval_nba(VL1DCache___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VL1DCache___024root___eval_nba\n"); );
    VL1DCache__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    if ((1ULL & vlSelfRef.__VnbaTriggered.word(0U))) {
        VL1DCache___024root___nba_sequent__TOP__0(vlSelf);
    }
}

VL_INLINE_OPT void VL1DCache___024root___nba_sequent__TOP__0(VL1DCache___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VL1DCache___024root___nba_sequent__TOP__0\n"); );
    VL1DCache__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Init
    CData/*6:0*/ L1DCache__DOT__lru_tree_in_w;
    L1DCache__DOT__lru_tree_in_w = 0;
    CData/*2:0*/ L1DCache__DOT__lru_access_way_w;
    L1DCache__DOT__lru_access_way_w = 0;
    CData/*0:0*/ L1DCache__DOT__lru_access_en_w;
    L1DCache__DOT__lru_access_en_w = 0;
    CData/*6:0*/ L1DCache__DOT__lru_tree_out_w;
    L1DCache__DOT__lru_tree_out_w = 0;
    CData/*0:0*/ L1DCache__DOT__wb_done_w;
    L1DCache__DOT__wb_done_w = 0;
    QData/*63:0*/ L1DCache__DOT__wb_word_0_w;
    L1DCache__DOT__wb_word_0_w = 0;
    QData/*63:0*/ L1DCache__DOT__wb_word_1_w;
    L1DCache__DOT__wb_word_1_w = 0;
    QData/*63:0*/ L1DCache__DOT__wb_word_2_w;
    L1DCache__DOT__wb_word_2_w = 0;
    QData/*63:0*/ L1DCache__DOT__wb_word_3_w;
    L1DCache__DOT__wb_word_3_w = 0;
    QData/*63:0*/ L1DCache__DOT__wb_word_4_w;
    L1DCache__DOT__wb_word_4_w = 0;
    QData/*63:0*/ L1DCache__DOT__wb_word_5_w;
    L1DCache__DOT__wb_word_5_w = 0;
    QData/*63:0*/ L1DCache__DOT__wb_word_6_w;
    L1DCache__DOT__wb_word_6_w = 0;
    QData/*63:0*/ L1DCache__DOT__wb_word_7_w;
    L1DCache__DOT__wb_word_7_w = 0;
    CData/*0:0*/ L1DCache__DOT__req_ready_w;
    L1DCache__DOT__req_ready_w = 0;
    CData/*0:0*/ L1DCache__DOT__resp_valid_w;
    L1DCache__DOT__resp_valid_w = 0;
    QData/*63:0*/ L1DCache__DOT__resp_data_w;
    L1DCache__DOT__resp_data_w = 0;
    CData/*0:0*/ L1DCache__DOT__ar_valid_w;
    L1DCache__DOT__ar_valid_w = 0;
    QData/*63:0*/ L1DCache__DOT__ar_addr_w;
    L1DCache__DOT__ar_addr_w = 0;
    CData/*3:0*/ L1DCache__DOT__ar_id_w;
    L1DCache__DOT__ar_id_w = 0;
    CData/*7:0*/ L1DCache__DOT__ar_len_w;
    L1DCache__DOT__ar_len_w = 0;
    CData/*2:0*/ L1DCache__DOT__ar_size_w;
    L1DCache__DOT__ar_size_w = 0;
    CData/*1:0*/ L1DCache__DOT__ar_burst_w;
    L1DCache__DOT__ar_burst_w = 0;
    CData/*0:0*/ L1DCache__DOT__r_ready_w;
    L1DCache__DOT__r_ready_w = 0;
    CData/*0:0*/ L1DCache__DOT__aw_valid_w;
    L1DCache__DOT__aw_valid_w = 0;
    QData/*63:0*/ L1DCache__DOT__aw_addr_w;
    L1DCache__DOT__aw_addr_w = 0;
    CData/*3:0*/ L1DCache__DOT__aw_id_w;
    L1DCache__DOT__aw_id_w = 0;
    CData/*7:0*/ L1DCache__DOT__aw_len_w;
    L1DCache__DOT__aw_len_w = 0;
    CData/*2:0*/ L1DCache__DOT__aw_size_w;
    L1DCache__DOT__aw_size_w = 0;
    CData/*1:0*/ L1DCache__DOT__aw_burst_w;
    L1DCache__DOT__aw_burst_w = 0;
    CData/*0:0*/ L1DCache__DOT__w_valid_w;
    L1DCache__DOT__w_valid_w = 0;
    QData/*63:0*/ L1DCache__DOT__w_data_w;
    L1DCache__DOT__w_data_w = 0;
    CData/*7:0*/ L1DCache__DOT__w_strb_w;
    L1DCache__DOT__w_strb_w = 0;
    CData/*0:0*/ L1DCache__DOT__w_last_w;
    L1DCache__DOT__w_last_w = 0;
    CData/*0:0*/ L1DCache__DOT__b_ready_w;
    L1DCache__DOT__b_ready_w = 0;
    CData/*6:0*/ L1DCache__DOT__lru_upd__DOT__updated;
    L1DCache__DOT__lru_upd__DOT__updated = 0;
    IData/*31:0*/ L1DCache__DOT__lru_upd__DOT__step;
    L1DCache__DOT__lru_upd__DOT__step = 0;
    CData/*0:0*/ L1DCache__DOT__lru_upd__DOT__way_bit;
    L1DCache__DOT__lru_upd__DOT__way_bit = 0;
    CData/*0:0*/ L1DCache__DOT__lru_upd__DOT____Vlvbound_h6f501444__0;
    L1DCache__DOT__lru_upd__DOT____Vlvbound_h6f501444__0 = 0;
    CData/*5:0*/ __Vtableidx1;
    __Vtableidx1 = 0;
    SData/*9:0*/ __Vtableidx2;
    __Vtableidx2 = 0;
    CData/*3:0*/ __Vdly__L1DCache__DOT__ctrl__DOT__beat_ctr_r;
    __Vdly__L1DCache__DOT__ctrl__DOT__beat_ctr_r = 0;
    CData/*3:0*/ __Vdly__L1DCache__DOT__fill_fsm__DOT__beat_ctr_r;
    __Vdly__L1DCache__DOT__fill_fsm__DOT__beat_ctr_r = 0;
    CData/*3:0*/ __Vdly__L1DCache__DOT__wb_fsm__DOT__beat_ctr_r;
    __Vdly__L1DCache__DOT__wb_fsm__DOT__beat_ctr_r = 0;
    QData/*53:0*/ __VdlyVal__L1DCache__DOT__tag_0__DOT__mem__v0;
    __VdlyVal__L1DCache__DOT__tag_0__DOT__mem__v0 = 0;
    CData/*5:0*/ __VdlyDim0__L1DCache__DOT__tag_0__DOT__mem__v0;
    __VdlyDim0__L1DCache__DOT__tag_0__DOT__mem__v0 = 0;
    CData/*0:0*/ __VdlySet__L1DCache__DOT__tag_0__DOT__mem__v0;
    __VdlySet__L1DCache__DOT__tag_0__DOT__mem__v0 = 0;
    QData/*53:0*/ __VdlyVal__L1DCache__DOT__tag_1__DOT__mem__v0;
    __VdlyVal__L1DCache__DOT__tag_1__DOT__mem__v0 = 0;
    CData/*5:0*/ __VdlyDim0__L1DCache__DOT__tag_1__DOT__mem__v0;
    __VdlyDim0__L1DCache__DOT__tag_1__DOT__mem__v0 = 0;
    CData/*0:0*/ __VdlySet__L1DCache__DOT__tag_1__DOT__mem__v0;
    __VdlySet__L1DCache__DOT__tag_1__DOT__mem__v0 = 0;
    QData/*53:0*/ __VdlyVal__L1DCache__DOT__tag_2__DOT__mem__v0;
    __VdlyVal__L1DCache__DOT__tag_2__DOT__mem__v0 = 0;
    CData/*5:0*/ __VdlyDim0__L1DCache__DOT__tag_2__DOT__mem__v0;
    __VdlyDim0__L1DCache__DOT__tag_2__DOT__mem__v0 = 0;
    CData/*0:0*/ __VdlySet__L1DCache__DOT__tag_2__DOT__mem__v0;
    __VdlySet__L1DCache__DOT__tag_2__DOT__mem__v0 = 0;
    QData/*53:0*/ __VdlyVal__L1DCache__DOT__tag_3__DOT__mem__v0;
    __VdlyVal__L1DCache__DOT__tag_3__DOT__mem__v0 = 0;
    CData/*5:0*/ __VdlyDim0__L1DCache__DOT__tag_3__DOT__mem__v0;
    __VdlyDim0__L1DCache__DOT__tag_3__DOT__mem__v0 = 0;
    CData/*0:0*/ __VdlySet__L1DCache__DOT__tag_3__DOT__mem__v0;
    __VdlySet__L1DCache__DOT__tag_3__DOT__mem__v0 = 0;
    QData/*53:0*/ __VdlyVal__L1DCache__DOT__tag_4__DOT__mem__v0;
    __VdlyVal__L1DCache__DOT__tag_4__DOT__mem__v0 = 0;
    CData/*5:0*/ __VdlyDim0__L1DCache__DOT__tag_4__DOT__mem__v0;
    __VdlyDim0__L1DCache__DOT__tag_4__DOT__mem__v0 = 0;
    CData/*0:0*/ __VdlySet__L1DCache__DOT__tag_4__DOT__mem__v0;
    __VdlySet__L1DCache__DOT__tag_4__DOT__mem__v0 = 0;
    QData/*53:0*/ __VdlyVal__L1DCache__DOT__tag_5__DOT__mem__v0;
    __VdlyVal__L1DCache__DOT__tag_5__DOT__mem__v0 = 0;
    CData/*5:0*/ __VdlyDim0__L1DCache__DOT__tag_5__DOT__mem__v0;
    __VdlyDim0__L1DCache__DOT__tag_5__DOT__mem__v0 = 0;
    CData/*0:0*/ __VdlySet__L1DCache__DOT__tag_5__DOT__mem__v0;
    __VdlySet__L1DCache__DOT__tag_5__DOT__mem__v0 = 0;
    QData/*53:0*/ __VdlyVal__L1DCache__DOT__tag_6__DOT__mem__v0;
    __VdlyVal__L1DCache__DOT__tag_6__DOT__mem__v0 = 0;
    CData/*5:0*/ __VdlyDim0__L1DCache__DOT__tag_6__DOT__mem__v0;
    __VdlyDim0__L1DCache__DOT__tag_6__DOT__mem__v0 = 0;
    CData/*0:0*/ __VdlySet__L1DCache__DOT__tag_6__DOT__mem__v0;
    __VdlySet__L1DCache__DOT__tag_6__DOT__mem__v0 = 0;
    QData/*53:0*/ __VdlyVal__L1DCache__DOT__tag_7__DOT__mem__v0;
    __VdlyVal__L1DCache__DOT__tag_7__DOT__mem__v0 = 0;
    CData/*5:0*/ __VdlyDim0__L1DCache__DOT__tag_7__DOT__mem__v0;
    __VdlyDim0__L1DCache__DOT__tag_7__DOT__mem__v0 = 0;
    CData/*0:0*/ __VdlySet__L1DCache__DOT__tag_7__DOT__mem__v0;
    __VdlySet__L1DCache__DOT__tag_7__DOT__mem__v0 = 0;
    QData/*63:0*/ __VdlyVal__L1DCache__DOT__data_ram__DOT__mem__v0;
    __VdlyVal__L1DCache__DOT__data_ram__DOT__mem__v0 = 0;
    SData/*11:0*/ __VdlyDim0__L1DCache__DOT__data_ram__DOT__mem__v0;
    __VdlyDim0__L1DCache__DOT__data_ram__DOT__mem__v0 = 0;
    CData/*0:0*/ __VdlySet__L1DCache__DOT__data_ram__DOT__mem__v0;
    __VdlySet__L1DCache__DOT__data_ram__DOT__mem__v0 = 0;
    CData/*6:0*/ __VdlyVal__L1DCache__DOT__lru_ram__DOT__mem__v0;
    __VdlyVal__L1DCache__DOT__lru_ram__DOT__mem__v0 = 0;
    CData/*5:0*/ __VdlyDim0__L1DCache__DOT__lru_ram__DOT__mem__v0;
    __VdlyDim0__L1DCache__DOT__lru_ram__DOT__mem__v0 = 0;
    CData/*0:0*/ __VdlySet__L1DCache__DOT__lru_ram__DOT__mem__v0;
    __VdlySet__L1DCache__DOT__lru_ram__DOT__mem__v0 = 0;
    // Body
    __VdlySet__L1DCache__DOT__lru_ram__DOT__mem__v0 = 0U;
    __VdlySet__L1DCache__DOT__tag_0__DOT__mem__v0 = 0U;
    __VdlySet__L1DCache__DOT__tag_1__DOT__mem__v0 = 0U;
    __VdlySet__L1DCache__DOT__tag_2__DOT__mem__v0 = 0U;
    __VdlySet__L1DCache__DOT__tag_3__DOT__mem__v0 = 0U;
    __VdlySet__L1DCache__DOT__tag_4__DOT__mem__v0 = 0U;
    __VdlySet__L1DCache__DOT__tag_5__DOT__mem__v0 = 0U;
    __VdlySet__L1DCache__DOT__tag_6__DOT__mem__v0 = 0U;
    __VdlySet__L1DCache__DOT__tag_7__DOT__mem__v0 = 0U;
    __VdlySet__L1DCache__DOT__data_ram__DOT__mem__v0 = 0U;
    __Vdly__L1DCache__DOT__wb_fsm__DOT__beat_ctr_r 
        = vlSelfRef.L1DCache__DOT__wb_fsm__DOT__beat_ctr_r;
    __Vdly__L1DCache__DOT__fill_fsm__DOT__beat_ctr_r 
        = vlSelfRef.L1DCache__DOT__fill_fsm__DOT__beat_ctr_r;
    __Vdly__L1DCache__DOT__ctrl__DOT__beat_ctr_r = vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r;
    if (vlSelfRef.L1DCache__DOT__lru_wr_en_w) {
        __VdlyVal__L1DCache__DOT__lru_ram__DOT__mem__v0 
            = vlSelfRef.L1DCache__DOT__lru_wr_data_w;
        __VdlyDim0__L1DCache__DOT__lru_ram__DOT__mem__v0 
            = vlSelfRef.L1DCache__DOT__lru_wr_addr_w;
        __VdlySet__L1DCache__DOT__lru_ram__DOT__mem__v0 = 1U;
    }
    if (vlSelfRef.L1DCache__DOT__tag_wr_en_0) {
        __VdlyVal__L1DCache__DOT__tag_0__DOT__mem__v0 
            = vlSelfRef.L1DCache__DOT__tag_wr_data_0;
        __VdlyDim0__L1DCache__DOT__tag_0__DOT__mem__v0 
            = vlSelfRef.L1DCache__DOT__tag_wr_addr_0;
        __VdlySet__L1DCache__DOT__tag_0__DOT__mem__v0 = 1U;
    }
    if (vlSelfRef.L1DCache__DOT__tag_wr_en_1) {
        __VdlyVal__L1DCache__DOT__tag_1__DOT__mem__v0 
            = vlSelfRef.L1DCache__DOT__tag_wr_data_1;
        __VdlyDim0__L1DCache__DOT__tag_1__DOT__mem__v0 
            = vlSelfRef.L1DCache__DOT__tag_wr_addr_1;
        __VdlySet__L1DCache__DOT__tag_1__DOT__mem__v0 = 1U;
    }
    if (vlSelfRef.L1DCache__DOT__tag_wr_en_2) {
        __VdlyVal__L1DCache__DOT__tag_2__DOT__mem__v0 
            = vlSelfRef.L1DCache__DOT__tag_wr_data_2;
        __VdlyDim0__L1DCache__DOT__tag_2__DOT__mem__v0 
            = vlSelfRef.L1DCache__DOT__tag_wr_addr_2;
        __VdlySet__L1DCache__DOT__tag_2__DOT__mem__v0 = 1U;
    }
    if (vlSelfRef.L1DCache__DOT__tag_wr_en_3) {
        __VdlyVal__L1DCache__DOT__tag_3__DOT__mem__v0 
            = vlSelfRef.L1DCache__DOT__tag_wr_data_3;
        __VdlyDim0__L1DCache__DOT__tag_3__DOT__mem__v0 
            = vlSelfRef.L1DCache__DOT__tag_wr_addr_3;
        __VdlySet__L1DCache__DOT__tag_3__DOT__mem__v0 = 1U;
    }
    if (vlSelfRef.L1DCache__DOT__tag_wr_en_4) {
        __VdlyVal__L1DCache__DOT__tag_4__DOT__mem__v0 
            = vlSelfRef.L1DCache__DOT__tag_wr_data_4;
        __VdlyDim0__L1DCache__DOT__tag_4__DOT__mem__v0 
            = vlSelfRef.L1DCache__DOT__tag_wr_addr_4;
        __VdlySet__L1DCache__DOT__tag_4__DOT__mem__v0 = 1U;
    }
    if (vlSelfRef.L1DCache__DOT__tag_wr_en_5) {
        __VdlyVal__L1DCache__DOT__tag_5__DOT__mem__v0 
            = vlSelfRef.L1DCache__DOT__tag_wr_data_5;
        __VdlyDim0__L1DCache__DOT__tag_5__DOT__mem__v0 
            = vlSelfRef.L1DCache__DOT__tag_wr_addr_5;
        __VdlySet__L1DCache__DOT__tag_5__DOT__mem__v0 = 1U;
    }
    if (vlSelfRef.L1DCache__DOT__tag_wr_en_6) {
        __VdlyVal__L1DCache__DOT__tag_6__DOT__mem__v0 
            = vlSelfRef.L1DCache__DOT__tag_wr_data_6;
        __VdlyDim0__L1DCache__DOT__tag_6__DOT__mem__v0 
            = vlSelfRef.L1DCache__DOT__tag_wr_addr_6;
        __VdlySet__L1DCache__DOT__tag_6__DOT__mem__v0 = 1U;
    }
    if (vlSelfRef.L1DCache__DOT__tag_wr_en_7) {
        __VdlyVal__L1DCache__DOT__tag_7__DOT__mem__v0 
            = vlSelfRef.L1DCache__DOT__tag_wr_data_7;
        __VdlyDim0__L1DCache__DOT__tag_7__DOT__mem__v0 
            = vlSelfRef.L1DCache__DOT__tag_wr_addr_7;
        __VdlySet__L1DCache__DOT__tag_7__DOT__mem__v0 = 1U;
    }
    if (vlSelfRef.L1DCache__DOT__data_wr_en_w) {
        __VdlyVal__L1DCache__DOT__data_ram__DOT__mem__v0 
            = vlSelfRef.L1DCache__DOT__data_wr_data_w;
        __VdlyDim0__L1DCache__DOT__data_ram__DOT__mem__v0 
            = vlSelfRef.L1DCache__DOT__data_wr_addr_w;
        __VdlySet__L1DCache__DOT__data_ram__DOT__mem__v0 = 1U;
    }
    if (vlSelfRef.rst) {
        vlSelfRef.L1DCache__DOT__wb_fsm__DOT__wb_addr_r = 0ULL;
        __Vdly__L1DCache__DOT__wb_fsm__DOT__beat_ctr_r = 0U;
        vlSelfRef.L1DCache__DOT__fill_fsm__DOT__fill_addr_r = 0ULL;
        __Vdly__L1DCache__DOT__fill_fsm__DOT__beat_ctr_r = 0U;
        __Vdly__L1DCache__DOT__ctrl__DOT__beat_ctr_r = 0U;
        vlSelfRef.L1DCache__DOT__wb_fsm__DOT__state_r = 0U;
        vlSelfRef.L1DCache__DOT__fill_fsm__DOT__state_r = 0U;
        vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r = 0U;
    } else {
        if ((0U == (IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__state_r))) {
            if (vlSelfRef.L1DCache__DOT__wb_start_w) {
                vlSelfRef.L1DCache__DOT__wb_fsm__DOT__wb_addr_r 
                    = vlSelfRef.L1DCache__DOT__wb_addr_w;
                __Vdly__L1DCache__DOT__wb_fsm__DOT__beat_ctr_r = 0U;
            }
        } else if ((2U == (IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__state_r))) {
            if (vlSelfRef.w_ready) {
                __Vdly__L1DCache__DOT__wb_fsm__DOT__beat_ctr_r 
                    = (0xfU & ((IData)(1U) + (IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__beat_ctr_r)));
            }
        }
        if ((0U == (IData)(vlSelfRef.L1DCache__DOT__fill_fsm__DOT__state_r))) {
            if (vlSelfRef.L1DCache__DOT__fill_start_w) {
                vlSelfRef.L1DCache__DOT__fill_fsm__DOT__fill_addr_r 
                    = vlSelfRef.L1DCache__DOT__fill_addr_w;
                __Vdly__L1DCache__DOT__fill_fsm__DOT__beat_ctr_r = 0U;
            }
        } else if ((2U == (IData)(vlSelfRef.L1DCache__DOT__fill_fsm__DOT__state_r))) {
            if (vlSelfRef.r_valid) {
                if ((0U == (IData)(vlSelfRef.L1DCache__DOT__fill_fsm__DOT__beat_ctr_r))) {
                    vlSelfRef.L1DCache__DOT__fill_word_0_w 
                        = vlSelfRef.r_data;
                } else if ((1U == (IData)(vlSelfRef.L1DCache__DOT__fill_fsm__DOT__beat_ctr_r))) {
                    vlSelfRef.L1DCache__DOT__fill_word_1_w 
                        = vlSelfRef.r_data;
                } else if ((2U == (IData)(vlSelfRef.L1DCache__DOT__fill_fsm__DOT__beat_ctr_r))) {
                    vlSelfRef.L1DCache__DOT__fill_word_2_w 
                        = vlSelfRef.r_data;
                } else if ((3U == (IData)(vlSelfRef.L1DCache__DOT__fill_fsm__DOT__beat_ctr_r))) {
                    vlSelfRef.L1DCache__DOT__fill_word_3_w 
                        = vlSelfRef.r_data;
                } else if ((4U == (IData)(vlSelfRef.L1DCache__DOT__fill_fsm__DOT__beat_ctr_r))) {
                    vlSelfRef.L1DCache__DOT__fill_word_4_w 
                        = vlSelfRef.r_data;
                } else if ((5U == (IData)(vlSelfRef.L1DCache__DOT__fill_fsm__DOT__beat_ctr_r))) {
                    vlSelfRef.L1DCache__DOT__fill_word_5_w 
                        = vlSelfRef.r_data;
                } else if ((6U == (IData)(vlSelfRef.L1DCache__DOT__fill_fsm__DOT__beat_ctr_r))) {
                    vlSelfRef.L1DCache__DOT__fill_word_6_w 
                        = vlSelfRef.r_data;
                } else if ((7U == (IData)(vlSelfRef.L1DCache__DOT__fill_fsm__DOT__beat_ctr_r))) {
                    vlSelfRef.L1DCache__DOT__fill_word_7_w 
                        = vlSelfRef.r_data;
                }
                __Vdly__L1DCache__DOT__fill_fsm__DOT__beat_ctr_r 
                    = (0xfU & ((IData)(1U) + (IData)(vlSelfRef.L1DCache__DOT__fill_fsm__DOT__beat_ctr_r)));
            }
        }
        if ((8U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
            if ((1U & (~ ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r) 
                          >> 2U)))) {
                if ((1U & (~ ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r) 
                              >> 1U)))) {
                    if ((1U & (~ (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r)))) {
                        __Vdly__L1DCache__DOT__ctrl__DOT__beat_ctr_r = 0U;
                    }
                }
            }
        } else if ((4U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
            if ((2U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
                if ((1U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
                    __Vdly__L1DCache__DOT__ctrl__DOT__beat_ctr_r 
                        = (0xfU & ((IData)(1U) + (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r)));
                    if ((1U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                        vlSelfRef.L1DCache__DOT__ctrl__DOT__wb_buf_0 
                            = vlSelfRef.L1DCache__DOT__data_ram__DOT__rd_port_rdata_r;
                    } else if ((2U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                        vlSelfRef.L1DCache__DOT__ctrl__DOT__wb_buf_1 
                            = vlSelfRef.L1DCache__DOT__data_ram__DOT__rd_port_rdata_r;
                    } else if ((3U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                        vlSelfRef.L1DCache__DOT__ctrl__DOT__wb_buf_2 
                            = vlSelfRef.L1DCache__DOT__data_ram__DOT__rd_port_rdata_r;
                    } else if ((4U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                        vlSelfRef.L1DCache__DOT__ctrl__DOT__wb_buf_3 
                            = vlSelfRef.L1DCache__DOT__data_ram__DOT__rd_port_rdata_r;
                    } else if ((5U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                        vlSelfRef.L1DCache__DOT__ctrl__DOT__wb_buf_4 
                            = vlSelfRef.L1DCache__DOT__data_ram__DOT__rd_port_rdata_r;
                    } else if ((6U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                        vlSelfRef.L1DCache__DOT__ctrl__DOT__wb_buf_5 
                            = vlSelfRef.L1DCache__DOT__data_ram__DOT__rd_port_rdata_r;
                    } else if ((7U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                        vlSelfRef.L1DCache__DOT__ctrl__DOT__wb_buf_6 
                            = vlSelfRef.L1DCache__DOT__data_ram__DOT__rd_port_rdata_r;
                    } else if ((8U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                        vlSelfRef.L1DCache__DOT__ctrl__DOT__wb_buf_7 
                            = vlSelfRef.L1DCache__DOT__data_ram__DOT__rd_port_rdata_r;
                    }
                }
            } else {
                __Vdly__L1DCache__DOT__ctrl__DOT__beat_ctr_r 
                    = ((1U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))
                        ? (0xfU & ((IData)(1U) + (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r)))
                        : 0U);
            }
        } else if ((1U & (~ ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r) 
                             >> 1U)))) {
            if ((1U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
                vlSelfRef.L1DCache__DOT__ctrl__DOT__hit_way_r = 0U;
                vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_hit_r 
                    = ((((((((((0xfffffffffffffULL 
                                & (vlSelfRef.L1DCache__DOT__tag_0__DOT__rd_port_rdata_r 
                                   >> 2U)) == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                               >> 0xcU)) 
                              & (IData)(vlSelfRef.L1DCache__DOT__tag_0__DOT__rd_port_rdata_r)) 
                             | (((0xfffffffffffffULL 
                                  & (vlSelfRef.L1DCache__DOT__tag_1__DOT__rd_port_rdata_r 
                                     >> 2U)) == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                 >> 0xcU)) 
                                & (IData)(vlSelfRef.L1DCache__DOT__tag_1__DOT__rd_port_rdata_r))) 
                            | (((0xfffffffffffffULL 
                                 & (vlSelfRef.L1DCache__DOT__tag_2__DOT__rd_port_rdata_r 
                                    >> 2U)) == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                >> 0xcU)) 
                               & (IData)(vlSelfRef.L1DCache__DOT__tag_2__DOT__rd_port_rdata_r))) 
                           | (((0xfffffffffffffULL 
                                & (vlSelfRef.L1DCache__DOT__tag_3__DOT__rd_port_rdata_r 
                                   >> 2U)) == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                               >> 0xcU)) 
                              & (IData)(vlSelfRef.L1DCache__DOT__tag_3__DOT__rd_port_rdata_r))) 
                          | (((0xfffffffffffffULL & 
                               (vlSelfRef.L1DCache__DOT__tag_4__DOT__rd_port_rdata_r 
                                >> 2U)) == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                            >> 0xcU)) 
                             & (IData)(vlSelfRef.L1DCache__DOT__tag_4__DOT__rd_port_rdata_r))) 
                         | (((0xfffffffffffffULL & 
                              (vlSelfRef.L1DCache__DOT__tag_5__DOT__rd_port_rdata_r 
                               >> 2U)) == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                           >> 0xcU)) 
                            & (IData)(vlSelfRef.L1DCache__DOT__tag_5__DOT__rd_port_rdata_r))) 
                        | (((0xfffffffffffffULL & (vlSelfRef.L1DCache__DOT__tag_6__DOT__rd_port_rdata_r 
                                                   >> 2U)) 
                            == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                >> 0xcU)) & (IData)(vlSelfRef.L1DCache__DOT__tag_6__DOT__rd_port_rdata_r))) 
                       | (((0xfffffffffffffULL & (vlSelfRef.L1DCache__DOT__tag_7__DOT__rd_port_rdata_r 
                                                  >> 2U)) 
                           == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                               >> 0xcU)) & (IData)(vlSelfRef.L1DCache__DOT__tag_7__DOT__rd_port_rdata_r)));
                vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r 
                    = vlSelfRef.L1DCache__DOT__lru_upd__DOT__idx;
                vlSelfRef.L1DCache__DOT__ctrl__DOT__lru_tree_r 
                    = vlSelfRef.L1DCache__DOT__lru_ram__DOT__rd_port_rdata_r;
                vlSelfRef.L1DCache__DOT__ctrl__DOT__miss_is_store_r 
                    = vlSelfRef.L1DCache__DOT__ctrl__DOT__req_is_store_r;
                vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_tag_r = 0ULL;
                vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_victim_dirty_r = 0U;
                if ((((0xfffffffffffffULL & (vlSelfRef.L1DCache__DOT__tag_1__DOT__rd_port_rdata_r 
                                             >> 2U)) 
                      == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                          >> 0xcU)) & (IData)(vlSelfRef.L1DCache__DOT__tag_1__DOT__rd_port_rdata_r))) {
                    vlSelfRef.L1DCache__DOT__ctrl__DOT__hit_way_r = 1U;
                } else if ((((0xfffffffffffffULL & 
                              (vlSelfRef.L1DCache__DOT__tag_2__DOT__rd_port_rdata_r 
                               >> 2U)) == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                           >> 0xcU)) 
                            & (IData)(vlSelfRef.L1DCache__DOT__tag_2__DOT__rd_port_rdata_r))) {
                    vlSelfRef.L1DCache__DOT__ctrl__DOT__hit_way_r = 2U;
                } else if ((((0xfffffffffffffULL & 
                              (vlSelfRef.L1DCache__DOT__tag_3__DOT__rd_port_rdata_r 
                               >> 2U)) == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                           >> 0xcU)) 
                            & (IData)(vlSelfRef.L1DCache__DOT__tag_3__DOT__rd_port_rdata_r))) {
                    vlSelfRef.L1DCache__DOT__ctrl__DOT__hit_way_r = 3U;
                } else if ((((0xfffffffffffffULL & 
                              (vlSelfRef.L1DCache__DOT__tag_4__DOT__rd_port_rdata_r 
                               >> 2U)) == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                           >> 0xcU)) 
                            & (IData)(vlSelfRef.L1DCache__DOT__tag_4__DOT__rd_port_rdata_r))) {
                    vlSelfRef.L1DCache__DOT__ctrl__DOT__hit_way_r = 4U;
                } else if ((((0xfffffffffffffULL & 
                              (vlSelfRef.L1DCache__DOT__tag_5__DOT__rd_port_rdata_r 
                               >> 2U)) == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                           >> 0xcU)) 
                            & (IData)(vlSelfRef.L1DCache__DOT__tag_5__DOT__rd_port_rdata_r))) {
                    vlSelfRef.L1DCache__DOT__ctrl__DOT__hit_way_r = 5U;
                } else if ((((0xfffffffffffffULL & 
                              (vlSelfRef.L1DCache__DOT__tag_6__DOT__rd_port_rdata_r 
                               >> 2U)) == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                           >> 0xcU)) 
                            & (IData)(vlSelfRef.L1DCache__DOT__tag_6__DOT__rd_port_rdata_r))) {
                    vlSelfRef.L1DCache__DOT__ctrl__DOT__hit_way_r = 6U;
                } else if ((((0xfffffffffffffULL & 
                              (vlSelfRef.L1DCache__DOT__tag_7__DOT__rd_port_rdata_r 
                               >> 2U)) == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                           >> 0xcU)) 
                            & (IData)(vlSelfRef.L1DCache__DOT__tag_7__DOT__rd_port_rdata_r))) {
                    vlSelfRef.L1DCache__DOT__ctrl__DOT__hit_way_r = 7U;
                }
                if ((0U == (IData)(vlSelfRef.L1DCache__DOT__lru_upd__DOT__idx))) {
                    vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_tag_r 
                        = (0xfffffffffffffULL & (vlSelfRef.L1DCache__DOT__tag_0__DOT__rd_port_rdata_r 
                                                 >> 2U));
                    vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_victim_dirty_r 
                        = (1U & (IData)((vlSelfRef.L1DCache__DOT__tag_0__DOT__rd_port_rdata_r 
                                         >> 1U)));
                } else if ((1U == (IData)(vlSelfRef.L1DCache__DOT__lru_upd__DOT__idx))) {
                    vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_tag_r 
                        = (0xfffffffffffffULL & (vlSelfRef.L1DCache__DOT__tag_1__DOT__rd_port_rdata_r 
                                                 >> 2U));
                    vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_victim_dirty_r 
                        = (1U & (IData)((vlSelfRef.L1DCache__DOT__tag_1__DOT__rd_port_rdata_r 
                                         >> 1U)));
                } else if ((2U == (IData)(vlSelfRef.L1DCache__DOT__lru_upd__DOT__idx))) {
                    vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_tag_r 
                        = (0xfffffffffffffULL & (vlSelfRef.L1DCache__DOT__tag_2__DOT__rd_port_rdata_r 
                                                 >> 2U));
                    vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_victim_dirty_r 
                        = (1U & (IData)((vlSelfRef.L1DCache__DOT__tag_2__DOT__rd_port_rdata_r 
                                         >> 1U)));
                } else if ((3U == (IData)(vlSelfRef.L1DCache__DOT__lru_upd__DOT__idx))) {
                    vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_tag_r 
                        = (0xfffffffffffffULL & (vlSelfRef.L1DCache__DOT__tag_3__DOT__rd_port_rdata_r 
                                                 >> 2U));
                    vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_victim_dirty_r 
                        = (1U & (IData)((vlSelfRef.L1DCache__DOT__tag_3__DOT__rd_port_rdata_r 
                                         >> 1U)));
                } else if ((4U == (IData)(vlSelfRef.L1DCache__DOT__lru_upd__DOT__idx))) {
                    vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_tag_r 
                        = (0xfffffffffffffULL & (vlSelfRef.L1DCache__DOT__tag_4__DOT__rd_port_rdata_r 
                                                 >> 2U));
                    vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_victim_dirty_r 
                        = (1U & (IData)((vlSelfRef.L1DCache__DOT__tag_4__DOT__rd_port_rdata_r 
                                         >> 1U)));
                } else if ((5U == (IData)(vlSelfRef.L1DCache__DOT__lru_upd__DOT__idx))) {
                    vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_tag_r 
                        = (0xfffffffffffffULL & (vlSelfRef.L1DCache__DOT__tag_5__DOT__rd_port_rdata_r 
                                                 >> 2U));
                    vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_victim_dirty_r 
                        = (1U & (IData)((vlSelfRef.L1DCache__DOT__tag_5__DOT__rd_port_rdata_r 
                                         >> 1U)));
                } else if ((6U == (IData)(vlSelfRef.L1DCache__DOT__lru_upd__DOT__idx))) {
                    vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_tag_r 
                        = (0xfffffffffffffULL & (vlSelfRef.L1DCache__DOT__tag_6__DOT__rd_port_rdata_r 
                                                 >> 2U));
                    vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_victim_dirty_r 
                        = (1U & (IData)((vlSelfRef.L1DCache__DOT__tag_6__DOT__rd_port_rdata_r 
                                         >> 1U)));
                } else if ((7U == (IData)(vlSelfRef.L1DCache__DOT__lru_upd__DOT__idx))) {
                    vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_tag_r 
                        = (0xfffffffffffffULL & (vlSelfRef.L1DCache__DOT__tag_7__DOT__rd_port_rdata_r 
                                                 >> 2U));
                    vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_victim_dirty_r 
                        = (1U & (IData)((vlSelfRef.L1DCache__DOT__tag_7__DOT__rd_port_rdata_r 
                                         >> 1U)));
                }
            } else if (vlSelfRef.req_valid) {
                vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                    = vlSelfRef.req_vaddr;
                vlSelfRef.L1DCache__DOT__ctrl__DOT__req_data_r 
                    = vlSelfRef.req_data;
                vlSelfRef.L1DCache__DOT__ctrl__DOT__req_is_store_r 
                    = vlSelfRef.req_is_store;
                __Vdly__L1DCache__DOT__ctrl__DOT__beat_ctr_r = 0U;
            }
        }
        vlSelfRef.L1DCache__DOT__wb_fsm__DOT__state_r 
            = vlSelfRef.L1DCache__DOT__wb_fsm__DOT__state_next;
        vlSelfRef.L1DCache__DOT__fill_fsm__DOT__state_r 
            = vlSelfRef.L1DCache__DOT__fill_fsm__DOT__state_next;
        vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r 
            = vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next;
    }
    vlSelfRef.L1DCache__DOT__wb_fsm__DOT__beat_ctr_r 
        = __Vdly__L1DCache__DOT__wb_fsm__DOT__beat_ctr_r;
    vlSelfRef.L1DCache__DOT__fill_fsm__DOT__beat_ctr_r 
        = __Vdly__L1DCache__DOT__fill_fsm__DOT__beat_ctr_r;
    vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r 
        = __Vdly__L1DCache__DOT__ctrl__DOT__beat_ctr_r;
    if (vlSelfRef.L1DCache__DOT__tag_rd_en_0) {
        vlSelfRef.L1DCache__DOT__tag_0__DOT__rd_port_rdata_r 
            = vlSelfRef.L1DCache__DOT__tag_0__DOT__mem
            [vlSelfRef.L1DCache__DOT__tag_rd_addr_0];
    }
    if (vlSelfRef.L1DCache__DOT__tag_rd_en_1) {
        vlSelfRef.L1DCache__DOT__tag_1__DOT__rd_port_rdata_r 
            = vlSelfRef.L1DCache__DOT__tag_1__DOT__mem
            [vlSelfRef.L1DCache__DOT__tag_rd_addr_1];
    }
    if (vlSelfRef.L1DCache__DOT__tag_rd_en_2) {
        vlSelfRef.L1DCache__DOT__tag_2__DOT__rd_port_rdata_r 
            = vlSelfRef.L1DCache__DOT__tag_2__DOT__mem
            [vlSelfRef.L1DCache__DOT__tag_rd_addr_2];
    }
    if (vlSelfRef.L1DCache__DOT__tag_rd_en_3) {
        vlSelfRef.L1DCache__DOT__tag_3__DOT__rd_port_rdata_r 
            = vlSelfRef.L1DCache__DOT__tag_3__DOT__mem
            [vlSelfRef.L1DCache__DOT__tag_rd_addr_3];
    }
    if (vlSelfRef.L1DCache__DOT__tag_rd_en_4) {
        vlSelfRef.L1DCache__DOT__tag_4__DOT__rd_port_rdata_r 
            = vlSelfRef.L1DCache__DOT__tag_4__DOT__mem
            [vlSelfRef.L1DCache__DOT__tag_rd_addr_4];
    }
    if (vlSelfRef.L1DCache__DOT__tag_rd_en_5) {
        vlSelfRef.L1DCache__DOT__tag_5__DOT__rd_port_rdata_r 
            = vlSelfRef.L1DCache__DOT__tag_5__DOT__mem
            [vlSelfRef.L1DCache__DOT__tag_rd_addr_5];
    }
    if (vlSelfRef.L1DCache__DOT__tag_rd_en_6) {
        vlSelfRef.L1DCache__DOT__tag_6__DOT__rd_port_rdata_r 
            = vlSelfRef.L1DCache__DOT__tag_6__DOT__mem
            [vlSelfRef.L1DCache__DOT__tag_rd_addr_6];
    }
    if (vlSelfRef.L1DCache__DOT__tag_rd_en_7) {
        vlSelfRef.L1DCache__DOT__tag_7__DOT__rd_port_rdata_r 
            = vlSelfRef.L1DCache__DOT__tag_7__DOT__mem
            [vlSelfRef.L1DCache__DOT__tag_rd_addr_7];
    }
    if (vlSelfRef.L1DCache__DOT__data_rd_en_w) {
        vlSelfRef.L1DCache__DOT__data_ram__DOT__rd_port_rdata_r 
            = vlSelfRef.L1DCache__DOT__data_ram__DOT__mem
            [vlSelfRef.L1DCache__DOT__data_rd_addr_w];
    }
    if (vlSelfRef.L1DCache__DOT__lru_rd_en_w) {
        vlSelfRef.L1DCache__DOT__lru_ram__DOT__rd_port_rdata_r 
            = vlSelfRef.L1DCache__DOT__lru_ram__DOT__mem
            [vlSelfRef.L1DCache__DOT__lru_rd_addr_w];
    }
    if (__VdlySet__L1DCache__DOT__tag_0__DOT__mem__v0) {
        vlSelfRef.L1DCache__DOT__tag_0__DOT__mem[__VdlyDim0__L1DCache__DOT__tag_0__DOT__mem__v0] 
            = __VdlyVal__L1DCache__DOT__tag_0__DOT__mem__v0;
    }
    if (__VdlySet__L1DCache__DOT__tag_1__DOT__mem__v0) {
        vlSelfRef.L1DCache__DOT__tag_1__DOT__mem[__VdlyDim0__L1DCache__DOT__tag_1__DOT__mem__v0] 
            = __VdlyVal__L1DCache__DOT__tag_1__DOT__mem__v0;
    }
    if (__VdlySet__L1DCache__DOT__tag_2__DOT__mem__v0) {
        vlSelfRef.L1DCache__DOT__tag_2__DOT__mem[__VdlyDim0__L1DCache__DOT__tag_2__DOT__mem__v0] 
            = __VdlyVal__L1DCache__DOT__tag_2__DOT__mem__v0;
    }
    if (__VdlySet__L1DCache__DOT__tag_3__DOT__mem__v0) {
        vlSelfRef.L1DCache__DOT__tag_3__DOT__mem[__VdlyDim0__L1DCache__DOT__tag_3__DOT__mem__v0] 
            = __VdlyVal__L1DCache__DOT__tag_3__DOT__mem__v0;
    }
    if (__VdlySet__L1DCache__DOT__tag_4__DOT__mem__v0) {
        vlSelfRef.L1DCache__DOT__tag_4__DOT__mem[__VdlyDim0__L1DCache__DOT__tag_4__DOT__mem__v0] 
            = __VdlyVal__L1DCache__DOT__tag_4__DOT__mem__v0;
    }
    if (__VdlySet__L1DCache__DOT__tag_5__DOT__mem__v0) {
        vlSelfRef.L1DCache__DOT__tag_5__DOT__mem[__VdlyDim0__L1DCache__DOT__tag_5__DOT__mem__v0] 
            = __VdlyVal__L1DCache__DOT__tag_5__DOT__mem__v0;
    }
    if (__VdlySet__L1DCache__DOT__tag_6__DOT__mem__v0) {
        vlSelfRef.L1DCache__DOT__tag_6__DOT__mem[__VdlyDim0__L1DCache__DOT__tag_6__DOT__mem__v0] 
            = __VdlyVal__L1DCache__DOT__tag_6__DOT__mem__v0;
    }
    if (__VdlySet__L1DCache__DOT__tag_7__DOT__mem__v0) {
        vlSelfRef.L1DCache__DOT__tag_7__DOT__mem[__VdlyDim0__L1DCache__DOT__tag_7__DOT__mem__v0] 
            = __VdlyVal__L1DCache__DOT__tag_7__DOT__mem__v0;
    }
    if (__VdlySet__L1DCache__DOT__data_ram__DOT__mem__v0) {
        vlSelfRef.L1DCache__DOT__data_ram__DOT__mem[__VdlyDim0__L1DCache__DOT__data_ram__DOT__mem__v0] 
            = __VdlyVal__L1DCache__DOT__data_ram__DOT__mem__v0;
    }
    if (__VdlySet__L1DCache__DOT__lru_ram__DOT__mem__v0) {
        vlSelfRef.L1DCache__DOT__lru_ram__DOT__mem[__VdlyDim0__L1DCache__DOT__lru_ram__DOT__mem__v0] 
            = __VdlyVal__L1DCache__DOT__lru_ram__DOT__mem__v0;
    }
    L1DCache__DOT__aw_valid_w = 0U;
    L1DCache__DOT__aw_id_w = 0U;
    L1DCache__DOT__aw_len_w = 0U;
    L1DCache__DOT__aw_size_w = 0U;
    L1DCache__DOT__aw_burst_w = 0U;
    L1DCache__DOT__w_valid_w = 0U;
    L1DCache__DOT__w_strb_w = 0U;
    L1DCache__DOT__b_ready_w = 0U;
    L1DCache__DOT__aw_addr_w = 0ULL;
    if ((1U & (~ ((IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__state_r) 
                  >> 1U)))) {
        if ((1U & (IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__state_r))) {
            L1DCache__DOT__aw_valid_w = 1U;
            L1DCache__DOT__aw_id_w = 1U;
            L1DCache__DOT__aw_len_w = 7U;
            L1DCache__DOT__aw_size_w = 3U;
            L1DCache__DOT__aw_burst_w = 1U;
            L1DCache__DOT__aw_addr_w = (0xffffffffffffffc0ULL 
                                        & vlSelfRef.L1DCache__DOT__wb_fsm__DOT__wb_addr_r);
        }
    }
    L1DCache__DOT__w_last_w = 0U;
    L1DCache__DOT__wb_done_w = 0U;
    L1DCache__DOT__ar_valid_w = 0U;
    L1DCache__DOT__ar_id_w = 0U;
    L1DCache__DOT__ar_len_w = 0U;
    L1DCache__DOT__ar_size_w = 0U;
    L1DCache__DOT__ar_burst_w = 0U;
    L1DCache__DOT__r_ready_w = 0U;
    L1DCache__DOT__ar_addr_w = 0ULL;
    if ((1U & (~ ((IData)(vlSelfRef.L1DCache__DOT__fill_fsm__DOT__state_r) 
                  >> 1U)))) {
        if ((1U & (IData)(vlSelfRef.L1DCache__DOT__fill_fsm__DOT__state_r))) {
            L1DCache__DOT__ar_valid_w = 1U;
            L1DCache__DOT__ar_id_w = 0U;
            L1DCache__DOT__ar_len_w = 7U;
            L1DCache__DOT__ar_size_w = 3U;
            L1DCache__DOT__ar_burst_w = 1U;
            L1DCache__DOT__ar_addr_w = (0xffffffffffffffc0ULL 
                                        & vlSelfRef.L1DCache__DOT__fill_fsm__DOT__fill_addr_r);
        }
    }
    vlSelfRef.L1DCache__DOT__fill_done_w = 0U;
    if ((2U & (IData)(vlSelfRef.L1DCache__DOT__fill_fsm__DOT__state_r))) {
        if ((1U & (~ (IData)(vlSelfRef.L1DCache__DOT__fill_fsm__DOT__state_r)))) {
            L1DCache__DOT__r_ready_w = 1U;
        }
        if ((1U & (IData)(vlSelfRef.L1DCache__DOT__fill_fsm__DOT__state_r))) {
            vlSelfRef.L1DCache__DOT__fill_done_w = 1U;
        }
    }
    vlSelfRef.L1DCache__DOT__tag_rd_en_0 = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_en_1 = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_en_2 = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_en_3 = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_en_4 = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_en_5 = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_en_6 = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_en_7 = 0U;
    vlSelfRef.L1DCache__DOT__lru_rd_en_w = 0U;
    vlSelfRef.L1DCache__DOT__lru_wr_en_w = 0U;
    vlSelfRef.L1DCache__DOT__fill_addr_w = 0ULL;
    vlSelfRef.L1DCache__DOT__tag_rd_addr_0 = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_addr_1 = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_addr_2 = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_addr_3 = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_addr_4 = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_addr_5 = 0U;
    vlSelfRef.L1DCache__DOT__tag_rd_addr_6 = 0U;
    vlSelfRef.L1DCache__DOT__data_wr_en_w = 0U;
    vlSelfRef.L1DCache__DOT__lru_rd_addr_w = 0U;
    vlSelfRef.L1DCache__DOT__lru_wr_addr_w = 0U;
    vlSelfRef.L1DCache__DOT__wb_addr_w = 0ULL;
    vlSelfRef.L1DCache__DOT__tag_rd_addr_7 = 0U;
    L1DCache__DOT__req_ready_w = 0U;
    vlSelfRef.L1DCache__DOT__tag_wr_en_0 = 0U;
    vlSelfRef.L1DCache__DOT__tag_wr_en_1 = 0U;
    vlSelfRef.L1DCache__DOT__tag_wr_en_2 = 0U;
    vlSelfRef.L1DCache__DOT__tag_wr_en_3 = 0U;
    vlSelfRef.L1DCache__DOT__tag_wr_en_4 = 0U;
    vlSelfRef.L1DCache__DOT__tag_wr_en_5 = 0U;
    vlSelfRef.L1DCache__DOT__tag_wr_en_6 = 0U;
    vlSelfRef.L1DCache__DOT__tag_wr_en_7 = 0U;
    vlSelfRef.L1DCache__DOT__tag_wr_addr_0 = 0U;
    vlSelfRef.L1DCache__DOT__tag_wr_data_0 = 0ULL;
    vlSelfRef.L1DCache__DOT__tag_wr_addr_1 = 0U;
    vlSelfRef.L1DCache__DOT__tag_wr_data_1 = 0ULL;
    vlSelfRef.L1DCache__DOT__tag_wr_addr_2 = 0U;
    vlSelfRef.L1DCache__DOT__tag_wr_data_2 = 0ULL;
    vlSelfRef.L1DCache__DOT__tag_wr_addr_3 = 0U;
    vlSelfRef.L1DCache__DOT__tag_wr_data_3 = 0ULL;
    vlSelfRef.L1DCache__DOT__tag_wr_addr_4 = 0U;
    vlSelfRef.L1DCache__DOT__tag_wr_data_4 = 0ULL;
    vlSelfRef.L1DCache__DOT__tag_wr_addr_5 = 0U;
    vlSelfRef.L1DCache__DOT__tag_wr_data_5 = 0ULL;
    vlSelfRef.L1DCache__DOT__tag_wr_addr_6 = 0U;
    vlSelfRef.L1DCache__DOT__tag_wr_data_6 = 0ULL;
    vlSelfRef.L1DCache__DOT__tag_wr_addr_7 = 0U;
    vlSelfRef.L1DCache__DOT__tag_wr_data_7 = 0ULL;
    L1DCache__DOT__resp_valid_w = 0U;
    vlSelfRef.L1DCache__DOT__data_wr_addr_w = 0U;
    vlSelfRef.L1DCache__DOT__data_rd_en_w = 0U;
    vlSelfRef.L1DCache__DOT__fill_start_w = 0U;
    vlSelfRef.L1DCache__DOT__data_rd_addr_w = 0U;
    L1DCache__DOT__lru_access_en_w = 0U;
    vlSelfRef.L1DCache__DOT__wb_start_w = 0U;
    vlSelfRef.L1DCache__DOT__data_wr_data_w = 0ULL;
    L1DCache__DOT__resp_data_w = 0ULL;
    L1DCache__DOT__wb_word_0_w = 0ULL;
    L1DCache__DOT__wb_word_1_w = 0ULL;
    L1DCache__DOT__wb_word_2_w = 0ULL;
    L1DCache__DOT__wb_word_3_w = 0ULL;
    L1DCache__DOT__wb_word_4_w = 0ULL;
    L1DCache__DOT__wb_word_5_w = 0ULL;
    L1DCache__DOT__wb_word_6_w = 0ULL;
    L1DCache__DOT__wb_word_7_w = 0ULL;
    L1DCache__DOT__lru_access_way_w = 0U;
    L1DCache__DOT__lru_tree_in_w = 0U;
    vlSelfRef.aw_valid = L1DCache__DOT__aw_valid_w;
    vlSelfRef.aw_id = L1DCache__DOT__aw_id_w;
    vlSelfRef.aw_len = L1DCache__DOT__aw_len_w;
    vlSelfRef.aw_size = L1DCache__DOT__aw_size_w;
    vlSelfRef.aw_burst = L1DCache__DOT__aw_burst_w;
    if ((2U & (IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__state_r))) {
        if ((1U & (~ (IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__state_r)))) {
            L1DCache__DOT__w_valid_w = 1U;
            L1DCache__DOT__w_strb_w = 0xffU;
            L1DCache__DOT__w_last_w = (7U == (IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__beat_ctr_r));
        }
        vlSelfRef.w_valid = L1DCache__DOT__w_valid_w;
        vlSelfRef.w_strb = L1DCache__DOT__w_strb_w;
        if ((1U & (IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__state_r))) {
            L1DCache__DOT__b_ready_w = 1U;
        }
        vlSelfRef.b_ready = L1DCache__DOT__b_ready_w;
        vlSelfRef.aw_addr = L1DCache__DOT__aw_addr_w;
    } else {
        vlSelfRef.w_valid = L1DCache__DOT__w_valid_w;
        vlSelfRef.w_strb = L1DCache__DOT__w_strb_w;
        vlSelfRef.b_ready = L1DCache__DOT__b_ready_w;
        vlSelfRef.aw_addr = L1DCache__DOT__aw_addr_w;
    }
    vlSelfRef.w_last = L1DCache__DOT__w_last_w;
    vlSelfRef.ar_valid = L1DCache__DOT__ar_valid_w;
    vlSelfRef.ar_id = L1DCache__DOT__ar_id_w;
    vlSelfRef.ar_len = L1DCache__DOT__ar_len_w;
    vlSelfRef.ar_size = L1DCache__DOT__ar_size_w;
    vlSelfRef.ar_burst = L1DCache__DOT__ar_burst_w;
    vlSelfRef.r_ready = L1DCache__DOT__r_ready_w;
    vlSelfRef.ar_addr = L1DCache__DOT__ar_addr_w;
    vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next 
        = vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r;
    if ((8U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
        if ((1U & (~ ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r) 
                      >> 2U)))) {
            if ((1U & (~ ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r) 
                          >> 1U)))) {
                if ((1U & (~ (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r)))) {
                    vlSelfRef.L1DCache__DOT__wb_addr_w 
                        = (VL_SHIFTL_QQI(64,64,32, vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_tag_r, 0xcU) 
                           | VL_SHIFTL_QQI(64,64,32, (QData)((IData)(
                                                                     (0x3fU 
                                                                      & (IData)(
                                                                                (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                                                >> 6U))))), 6U));
                    vlSelfRef.L1DCache__DOT__wb_start_w = 1U;
                    L1DCache__DOT__wb_word_0_w = vlSelfRef.L1DCache__DOT__ctrl__DOT__wb_buf_0;
                    L1DCache__DOT__wb_word_1_w = vlSelfRef.L1DCache__DOT__ctrl__DOT__wb_buf_1;
                    L1DCache__DOT__wb_word_2_w = vlSelfRef.L1DCache__DOT__ctrl__DOT__wb_buf_2;
                    L1DCache__DOT__wb_word_3_w = vlSelfRef.L1DCache__DOT__ctrl__DOT__wb_buf_3;
                    L1DCache__DOT__wb_word_4_w = vlSelfRef.L1DCache__DOT__ctrl__DOT__wb_buf_4;
                    L1DCache__DOT__wb_word_5_w = vlSelfRef.L1DCache__DOT__ctrl__DOT__wb_buf_5;
                    L1DCache__DOT__wb_word_6_w = vlSelfRef.L1DCache__DOT__ctrl__DOT__wb_buf_6;
                    L1DCache__DOT__wb_word_7_w = vlSelfRef.L1DCache__DOT__ctrl__DOT__wb_buf_7;
                }
            }
        }
    }
    if ((2U & (IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__state_r))) {
        if ((1U & (IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__state_r))) {
            L1DCache__DOT__wb_done_w = vlSelfRef.b_valid;
        }
    }
    if ((8U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
        if ((4U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
            vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next 
                = vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r;
        } else if ((2U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
            vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next 
                = vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r;
        } else if ((1U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
            vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next 
                = vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r;
        } else if (L1DCache__DOT__wb_done_w) {
            vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next = 3U;
        }
    } else if ((4U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
        if ((2U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
            if ((1U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
                if ((8U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                    vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next = 8U;
                }
            } else {
                vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next = 0U;
            }
        } else if ((1U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
            if (((7U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r)) 
                 & (~ (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__miss_is_store_r)))) {
                vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next = 0U;
            } else if (((7U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r)) 
                        & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__miss_is_store_r))) {
                vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next = 6U;
            }
        } else if (vlSelfRef.L1DCache__DOT__fill_done_w) {
            vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next = 5U;
        }
    } else if ((2U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
        if ((1U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
            vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next = 4U;
        } else if (vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_hit_r) {
            vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next = 0U;
        } else if (((~ (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_hit_r)) 
                    & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_victim_dirty_r))) {
            vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next = 7U;
        } else if ((1U & ((~ (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_hit_r)) 
                          & (~ (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_victim_dirty_r))))) {
            vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next = 3U;
        }
    } else if ((1U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
        vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next = 2U;
    } else if (vlSelfRef.req_valid) {
        vlSelfRef.L1DCache__DOT__ctrl__DOT__state_next = 1U;
    }
    if ((1U & (~ ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r) 
                  >> 3U)))) {
        if ((1U & (~ ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r) 
                      >> 2U)))) {
            if ((1U & (~ ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r) 
                          >> 1U)))) {
                if ((1U & (~ (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r)))) {
                    if (vlSelfRef.req_valid) {
                        vlSelfRef.L1DCache__DOT__tag_rd_en_0 = 1U;
                        vlSelfRef.L1DCache__DOT__tag_rd_en_1 = 1U;
                        vlSelfRef.L1DCache__DOT__tag_rd_en_2 = 1U;
                        vlSelfRef.L1DCache__DOT__tag_rd_en_3 = 1U;
                        vlSelfRef.L1DCache__DOT__tag_rd_en_4 = 1U;
                        vlSelfRef.L1DCache__DOT__tag_rd_en_5 = 1U;
                        vlSelfRef.L1DCache__DOT__tag_rd_en_6 = 1U;
                        vlSelfRef.L1DCache__DOT__tag_rd_en_7 = 1U;
                        vlSelfRef.L1DCache__DOT__lru_rd_en_w = 1U;
                        vlSelfRef.L1DCache__DOT__tag_rd_addr_0 
                            = (0x3fU & (IData)((vlSelfRef.req_vaddr 
                                                >> 6U)));
                        vlSelfRef.L1DCache__DOT__tag_rd_addr_1 
                            = (0x3fU & (IData)((vlSelfRef.req_vaddr 
                                                >> 6U)));
                        vlSelfRef.L1DCache__DOT__tag_rd_addr_2 
                            = (0x3fU & (IData)((vlSelfRef.req_vaddr 
                                                >> 6U)));
                        vlSelfRef.L1DCache__DOT__tag_rd_addr_3 
                            = (0x3fU & (IData)((vlSelfRef.req_vaddr 
                                                >> 6U)));
                        vlSelfRef.L1DCache__DOT__tag_rd_addr_4 
                            = (0x3fU & (IData)((vlSelfRef.req_vaddr 
                                                >> 6U)));
                        vlSelfRef.L1DCache__DOT__tag_rd_addr_5 
                            = (0x3fU & (IData)((vlSelfRef.req_vaddr 
                                                >> 6U)));
                        vlSelfRef.L1DCache__DOT__tag_rd_addr_6 
                            = (0x3fU & (IData)((vlSelfRef.req_vaddr 
                                                >> 6U)));
                        vlSelfRef.L1DCache__DOT__lru_rd_addr_w 
                            = (0x3fU & (IData)((vlSelfRef.req_vaddr 
                                                >> 6U)));
                        vlSelfRef.L1DCache__DOT__tag_rd_addr_7 
                            = (0x3fU & (IData)((vlSelfRef.req_vaddr 
                                                >> 6U)));
                    }
                    L1DCache__DOT__req_ready_w = 1U;
                }
            }
            if ((2U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
                if ((1U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
                    vlSelfRef.L1DCache__DOT__lru_wr_en_w = 1U;
                    vlSelfRef.L1DCache__DOT__fill_addr_w 
                        = vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r;
                    vlSelfRef.L1DCache__DOT__lru_wr_addr_w 
                        = (0x3fU & (IData)((vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                            >> 6U)));
                    vlSelfRef.L1DCache__DOT__fill_start_w = 1U;
                } else if (vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_hit_r) {
                    vlSelfRef.L1DCache__DOT__lru_wr_en_w = 1U;
                    vlSelfRef.L1DCache__DOT__lru_wr_addr_w 
                        = (0x3fU & (IData)((vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                            >> 6U)));
                }
            }
        }
        if ((4U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
            if ((2U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
                if ((1U & (~ (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r)))) {
                    vlSelfRef.L1DCache__DOT__data_wr_en_w = 1U;
                    if ((0U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                        vlSelfRef.L1DCache__DOT__tag_wr_en_0 = 1U;
                        vlSelfRef.L1DCache__DOT__tag_wr_addr_0 
                            = (0x3fU & (IData)((vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                >> 6U)));
                        vlSelfRef.L1DCache__DOT__tag_wr_data_0 
                            = (3ULL | (0x3ffffffffffffcULL 
                                       & (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                          >> 0xaU)));
                    }
                    if ((0U != (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                        if ((1U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                            vlSelfRef.L1DCache__DOT__tag_wr_en_1 = 1U;
                            vlSelfRef.L1DCache__DOT__tag_wr_addr_1 
                                = (0x3fU & (IData)(
                                                   (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                    >> 6U)));
                            vlSelfRef.L1DCache__DOT__tag_wr_data_1 
                                = (3ULL | (0x3ffffffffffffcULL 
                                           & (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                              >> 0xaU)));
                        }
                        if ((1U != (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                            if ((2U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                                vlSelfRef.L1DCache__DOT__tag_wr_en_2 = 1U;
                                vlSelfRef.L1DCache__DOT__tag_wr_addr_2 
                                    = (0x3fU & (IData)(
                                                       (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                        >> 6U)));
                                vlSelfRef.L1DCache__DOT__tag_wr_data_2 
                                    = (3ULL | (0x3ffffffffffffcULL 
                                               & (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                  >> 0xaU)));
                            }
                            if ((2U != (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                                if ((3U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                                    vlSelfRef.L1DCache__DOT__tag_wr_en_3 = 1U;
                                    vlSelfRef.L1DCache__DOT__tag_wr_addr_3 
                                        = (0x3fU & (IData)(
                                                           (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                            >> 6U)));
                                    vlSelfRef.L1DCache__DOT__tag_wr_data_3 
                                        = (3ULL | (0x3ffffffffffffcULL 
                                                   & (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                      >> 0xaU)));
                                }
                                if ((3U != (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                                    if ((4U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                                        vlSelfRef.L1DCache__DOT__tag_wr_en_4 = 1U;
                                        vlSelfRef.L1DCache__DOT__tag_wr_addr_4 
                                            = (0x3fU 
                                               & (IData)(
                                                         (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                          >> 6U)));
                                        vlSelfRef.L1DCache__DOT__tag_wr_data_4 
                                            = (3ULL 
                                               | (0x3ffffffffffffcULL 
                                                  & (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                     >> 0xaU)));
                                    }
                                    if ((4U != (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                                        if ((5U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                                            vlSelfRef.L1DCache__DOT__tag_wr_en_5 = 1U;
                                            vlSelfRef.L1DCache__DOT__tag_wr_addr_5 
                                                = (0x3fU 
                                                   & (IData)(
                                                             (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                              >> 6U)));
                                            vlSelfRef.L1DCache__DOT__tag_wr_data_5 
                                                = (3ULL 
                                                   | (0x3ffffffffffffcULL 
                                                      & (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                         >> 0xaU)));
                                        }
                                        if ((5U != (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                                            if ((6U 
                                                 == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                                                vlSelfRef.L1DCache__DOT__tag_wr_en_6 = 1U;
                                                vlSelfRef.L1DCache__DOT__tag_wr_addr_6 
                                                    = 
                                                    (0x3fU 
                                                     & (IData)(
                                                               (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                                >> 6U)));
                                                vlSelfRef.L1DCache__DOT__tag_wr_data_6 
                                                    = 
                                                    (3ULL 
                                                     | (0x3ffffffffffffcULL 
                                                        & (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                           >> 0xaU)));
                                            }
                                            if ((6U 
                                                 != (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                                                if (
                                                    (7U 
                                                     == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                                                    vlSelfRef.L1DCache__DOT__tag_wr_en_7 = 1U;
                                                    vlSelfRef.L1DCache__DOT__tag_wr_addr_7 
                                                        = 
                                                        (0x3fU 
                                                         & (IData)(
                                                                   (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                                    >> 6U)));
                                                    vlSelfRef.L1DCache__DOT__tag_wr_data_7 
                                                        = 
                                                        (3ULL 
                                                         | (0x3ffffffffffffcULL 
                                                            & (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                               >> 0xaU)));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    L1DCache__DOT__resp_valid_w = 1U;
                    vlSelfRef.L1DCache__DOT__data_wr_addr_w 
                        = ((0xfc0U & ((IData)((vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                               >> 6U)) 
                                      << 6U)) | (((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r) 
                                                  << 3U) 
                                                 | (7U 
                                                    & (IData)(
                                                              (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                               >> 3U)))));
                }
                if ((1U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
                    if ((0U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                        vlSelfRef.L1DCache__DOT__data_rd_en_w = 1U;
                    } else if ((1U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                        vlSelfRef.L1DCache__DOT__data_rd_en_w = 1U;
                    } else if ((2U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                        vlSelfRef.L1DCache__DOT__data_rd_en_w = 1U;
                    } else if ((3U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                        vlSelfRef.L1DCache__DOT__data_rd_en_w = 1U;
                    } else if ((4U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                        vlSelfRef.L1DCache__DOT__data_rd_en_w = 1U;
                    } else if ((5U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                        vlSelfRef.L1DCache__DOT__data_rd_en_w = 1U;
                    } else if ((6U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                        vlSelfRef.L1DCache__DOT__data_rd_en_w = 1U;
                    } else if ((7U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                        vlSelfRef.L1DCache__DOT__data_rd_en_w = 1U;
                    }
                }
            } else if ((1U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
                vlSelfRef.L1DCache__DOT__data_wr_en_w = 1U;
                if (((7U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r)) 
                     & (~ (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__miss_is_store_r)))) {
                    L1DCache__DOT__resp_valid_w = 1U;
                }
                vlSelfRef.L1DCache__DOT__data_wr_addr_w 
                    = ((0xfc0U & ((IData)((vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                           >> 6U)) 
                                  << 6U)) | ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r) 
                                             << 3U));
                if ((0U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                    vlSelfRef.L1DCache__DOT__data_wr_addr_w 
                        = ((0xfc0U & ((IData)((vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                               >> 6U)) 
                                      << 6U)) | ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r) 
                                                 << 3U));
                } else if ((1U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                    vlSelfRef.L1DCache__DOT__data_wr_addr_w 
                        = (1U | ((0xfc0U & ((IData)(
                                                    (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                     >> 6U)) 
                                            << 6U)) 
                                 | ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r) 
                                    << 3U)));
                } else if ((2U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                    vlSelfRef.L1DCache__DOT__data_wr_addr_w 
                        = (2U | ((0xfc0U & ((IData)(
                                                    (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                     >> 6U)) 
                                            << 6U)) 
                                 | ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r) 
                                    << 3U)));
                } else if ((3U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                    vlSelfRef.L1DCache__DOT__data_wr_addr_w 
                        = (3U | ((0xfc0U & ((IData)(
                                                    (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                     >> 6U)) 
                                            << 6U)) 
                                 | ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r) 
                                    << 3U)));
                } else if ((4U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                    vlSelfRef.L1DCache__DOT__data_wr_addr_w 
                        = (4U | ((0xfc0U & ((IData)(
                                                    (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                     >> 6U)) 
                                            << 6U)) 
                                 | ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r) 
                                    << 3U)));
                } else if ((5U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                    vlSelfRef.L1DCache__DOT__data_wr_addr_w 
                        = (5U | ((0xfc0U & ((IData)(
                                                    (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                     >> 6U)) 
                                            << 6U)) 
                                 | ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r) 
                                    << 3U)));
                } else if ((6U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                    vlSelfRef.L1DCache__DOT__data_wr_addr_w 
                        = (6U | ((0xfc0U & ((IData)(
                                                    (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                     >> 6U)) 
                                            << 6U)) 
                                 | ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r) 
                                    << 3U)));
                } else if ((7U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                    vlSelfRef.L1DCache__DOT__data_wr_addr_w 
                        = (7U | ((0xfc0U & ((IData)(
                                                    (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                     >> 6U)) 
                                            << 6U)) 
                                 | ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r) 
                                    << 3U)));
                }
            }
        } else {
            if ((2U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
                if ((1U & (~ (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r)))) {
                    if (vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_hit_r) {
                        if (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_is_store_r) {
                            vlSelfRef.L1DCache__DOT__data_wr_en_w = 1U;
                            vlSelfRef.L1DCache__DOT__data_wr_addr_w 
                                = ((0xfc0U & ((IData)(
                                                      (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                       >> 6U)) 
                                              << 6U)) 
                                   | (((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__hit_way_r) 
                                       << 3U) | (7U 
                                                 & (IData)(
                                                           (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                            >> 3U)))));
                        }
                        L1DCache__DOT__resp_valid_w = 1U;
                    } else {
                        L1DCache__DOT__resp_valid_w = 0U;
                    }
                }
                if ((1U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
                    if ((0U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                        vlSelfRef.L1DCache__DOT__tag_wr_en_0 = 1U;
                        vlSelfRef.L1DCache__DOT__tag_wr_addr_0 
                            = (0x3fU & (IData)((vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                >> 6U)));
                        vlSelfRef.L1DCache__DOT__tag_wr_data_0 
                            = (1ULL | (0x3ffffffffffffcULL 
                                       & (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                          >> 0xaU)));
                    }
                    if ((0U != (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                        if ((1U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                            vlSelfRef.L1DCache__DOT__tag_wr_en_1 = 1U;
                            vlSelfRef.L1DCache__DOT__tag_wr_addr_1 
                                = (0x3fU & (IData)(
                                                   (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                    >> 6U)));
                            vlSelfRef.L1DCache__DOT__tag_wr_data_1 
                                = (1ULL | (0x3ffffffffffffcULL 
                                           & (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                              >> 0xaU)));
                        }
                        if ((1U != (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                            if ((2U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                                vlSelfRef.L1DCache__DOT__tag_wr_en_2 = 1U;
                                vlSelfRef.L1DCache__DOT__tag_wr_addr_2 
                                    = (0x3fU & (IData)(
                                                       (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                        >> 6U)));
                                vlSelfRef.L1DCache__DOT__tag_wr_data_2 
                                    = (1ULL | (0x3ffffffffffffcULL 
                                               & (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                  >> 0xaU)));
                            }
                            if ((2U != (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                                if ((3U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                                    vlSelfRef.L1DCache__DOT__tag_wr_en_3 = 1U;
                                    vlSelfRef.L1DCache__DOT__tag_wr_addr_3 
                                        = (0x3fU & (IData)(
                                                           (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                            >> 6U)));
                                    vlSelfRef.L1DCache__DOT__tag_wr_data_3 
                                        = (1ULL | (0x3ffffffffffffcULL 
                                                   & (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                      >> 0xaU)));
                                }
                                if ((3U != (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                                    if ((4U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                                        vlSelfRef.L1DCache__DOT__tag_wr_en_4 = 1U;
                                        vlSelfRef.L1DCache__DOT__tag_wr_addr_4 
                                            = (0x3fU 
                                               & (IData)(
                                                         (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                          >> 6U)));
                                        vlSelfRef.L1DCache__DOT__tag_wr_data_4 
                                            = (1ULL 
                                               | (0x3ffffffffffffcULL 
                                                  & (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                     >> 0xaU)));
                                    }
                                    if ((4U != (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                                        if ((5U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                                            vlSelfRef.L1DCache__DOT__tag_wr_en_5 = 1U;
                                            vlSelfRef.L1DCache__DOT__tag_wr_addr_5 
                                                = (0x3fU 
                                                   & (IData)(
                                                             (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                              >> 6U)));
                                            vlSelfRef.L1DCache__DOT__tag_wr_data_5 
                                                = (1ULL 
                                                   | (0x3ffffffffffffcULL 
                                                      & (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                         >> 0xaU)));
                                        }
                                        if ((5U != (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                                            if ((6U 
                                                 == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                                                vlSelfRef.L1DCache__DOT__tag_wr_en_6 = 1U;
                                                vlSelfRef.L1DCache__DOT__tag_wr_addr_6 
                                                    = 
                                                    (0x3fU 
                                                     & (IData)(
                                                               (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                                >> 6U)));
                                                vlSelfRef.L1DCache__DOT__tag_wr_data_6 
                                                    = 
                                                    (1ULL 
                                                     | (0x3ffffffffffffcULL 
                                                        & (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                           >> 0xaU)));
                                            }
                                            if ((6U 
                                                 != (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                                                if (
                                                    (7U 
                                                     == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r))) {
                                                    vlSelfRef.L1DCache__DOT__tag_wr_en_7 = 1U;
                                                    vlSelfRef.L1DCache__DOT__tag_wr_addr_7 
                                                        = 
                                                        (0x3fU 
                                                         & (IData)(
                                                                   (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                                    >> 6U)));
                                                    vlSelfRef.L1DCache__DOT__tag_wr_data_7 
                                                        = 
                                                        (1ULL 
                                                         | (0x3ffffffffffffcULL 
                                                            & (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                               >> 0xaU)));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else if (vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_hit_r) {
                    if (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_is_store_r) {
                        if ((0U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__hit_way_r))) {
                            vlSelfRef.L1DCache__DOT__tag_wr_en_0 = 1U;
                            vlSelfRef.L1DCache__DOT__tag_wr_addr_0 
                                = (0x3fU & (IData)(
                                                   (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                    >> 6U)));
                            vlSelfRef.L1DCache__DOT__tag_wr_data_0 
                                = (3ULL | (0x3ffffffffffffcULL 
                                           & (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                              >> 0xaU)));
                        }
                        if ((0U != (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__hit_way_r))) {
                            if ((1U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__hit_way_r))) {
                                vlSelfRef.L1DCache__DOT__tag_wr_en_1 = 1U;
                                vlSelfRef.L1DCache__DOT__tag_wr_addr_1 
                                    = (0x3fU & (IData)(
                                                       (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                        >> 6U)));
                                vlSelfRef.L1DCache__DOT__tag_wr_data_1 
                                    = (3ULL | (0x3ffffffffffffcULL 
                                               & (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                  >> 0xaU)));
                            }
                            if ((1U != (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__hit_way_r))) {
                                if ((2U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__hit_way_r))) {
                                    vlSelfRef.L1DCache__DOT__tag_wr_en_2 = 1U;
                                    vlSelfRef.L1DCache__DOT__tag_wr_addr_2 
                                        = (0x3fU & (IData)(
                                                           (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                            >> 6U)));
                                    vlSelfRef.L1DCache__DOT__tag_wr_data_2 
                                        = (3ULL | (0x3ffffffffffffcULL 
                                                   & (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                      >> 0xaU)));
                                }
                                if ((2U != (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__hit_way_r))) {
                                    if ((3U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__hit_way_r))) {
                                        vlSelfRef.L1DCache__DOT__tag_wr_en_3 = 1U;
                                        vlSelfRef.L1DCache__DOT__tag_wr_addr_3 
                                            = (0x3fU 
                                               & (IData)(
                                                         (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                          >> 6U)));
                                        vlSelfRef.L1DCache__DOT__tag_wr_data_3 
                                            = (3ULL 
                                               | (0x3ffffffffffffcULL 
                                                  & (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                     >> 0xaU)));
                                    }
                                    if ((3U != (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__hit_way_r))) {
                                        if ((4U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__hit_way_r))) {
                                            vlSelfRef.L1DCache__DOT__tag_wr_en_4 = 1U;
                                            vlSelfRef.L1DCache__DOT__tag_wr_addr_4 
                                                = (0x3fU 
                                                   & (IData)(
                                                             (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                              >> 6U)));
                                            vlSelfRef.L1DCache__DOT__tag_wr_data_4 
                                                = (3ULL 
                                                   | (0x3ffffffffffffcULL 
                                                      & (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                         >> 0xaU)));
                                        }
                                        if ((4U != (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__hit_way_r))) {
                                            if ((5U 
                                                 == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__hit_way_r))) {
                                                vlSelfRef.L1DCache__DOT__tag_wr_en_5 = 1U;
                                                vlSelfRef.L1DCache__DOT__tag_wr_addr_5 
                                                    = 
                                                    (0x3fU 
                                                     & (IData)(
                                                               (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                                >> 6U)));
                                                vlSelfRef.L1DCache__DOT__tag_wr_data_5 
                                                    = 
                                                    (3ULL 
                                                     | (0x3ffffffffffffcULL 
                                                        & (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                           >> 0xaU)));
                                            }
                                            if ((5U 
                                                 != (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__hit_way_r))) {
                                                if (
                                                    (6U 
                                                     == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__hit_way_r))) {
                                                    vlSelfRef.L1DCache__DOT__tag_wr_en_6 = 1U;
                                                    vlSelfRef.L1DCache__DOT__tag_wr_addr_6 
                                                        = 
                                                        (0x3fU 
                                                         & (IData)(
                                                                   (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                                    >> 6U)));
                                                    vlSelfRef.L1DCache__DOT__tag_wr_data_6 
                                                        = 
                                                        (3ULL 
                                                         | (0x3ffffffffffffcULL 
                                                            & (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                               >> 0xaU)));
                                                }
                                                if (
                                                    (6U 
                                                     != (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__hit_way_r))) {
                                                    if (
                                                        (7U 
                                                         == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__hit_way_r))) {
                                                        vlSelfRef.L1DCache__DOT__tag_wr_en_7 = 1U;
                                                        vlSelfRef.L1DCache__DOT__tag_wr_addr_7 
                                                            = 
                                                            (0x3fU 
                                                             & (IData)(
                                                                       (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                                        >> 6U)));
                                                        vlSelfRef.L1DCache__DOT__tag_wr_data_7 
                                                            = 
                                                            (3ULL 
                                                             | (0x3ffffffffffffcULL 
                                                                & (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                                   >> 0xaU)));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if ((1U & (~ ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r) 
                          >> 1U)))) {
                if ((1U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
                    vlSelfRef.L1DCache__DOT__data_rd_en_w = 1U;
                    if ((1U & (~ (((0xfffffffffffffULL 
                                    & (vlSelfRef.L1DCache__DOT__tag_0__DOT__rd_port_rdata_r 
                                       >> 2U)) == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                   >> 0xcU)) 
                                  & (IData)(vlSelfRef.L1DCache__DOT__tag_0__DOT__rd_port_rdata_r))))) {
                        if ((1U & (~ (((0xfffffffffffffULL 
                                        & (vlSelfRef.L1DCache__DOT__tag_1__DOT__rd_port_rdata_r 
                                           >> 2U)) 
                                       == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                           >> 0xcU)) 
                                      & (IData)(vlSelfRef.L1DCache__DOT__tag_1__DOT__rd_port_rdata_r))))) {
                            if ((1U & (~ (((0xfffffffffffffULL 
                                            & (vlSelfRef.L1DCache__DOT__tag_2__DOT__rd_port_rdata_r 
                                               >> 2U)) 
                                           == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                               >> 0xcU)) 
                                          & (IData)(vlSelfRef.L1DCache__DOT__tag_2__DOT__rd_port_rdata_r))))) {
                                if ((1U & (~ (((0xfffffffffffffULL 
                                                & (vlSelfRef.L1DCache__DOT__tag_3__DOT__rd_port_rdata_r 
                                                   >> 2U)) 
                                               == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                   >> 0xcU)) 
                                              & (IData)(vlSelfRef.L1DCache__DOT__tag_3__DOT__rd_port_rdata_r))))) {
                                    if ((1U & (~ ((
                                                   (0xfffffffffffffULL 
                                                    & (vlSelfRef.L1DCache__DOT__tag_4__DOT__rd_port_rdata_r 
                                                       >> 2U)) 
                                                   == 
                                                   (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                    >> 0xcU)) 
                                                  & (IData)(vlSelfRef.L1DCache__DOT__tag_4__DOT__rd_port_rdata_r))))) {
                                        if ((1U & (~ 
                                                   (((0xfffffffffffffULL 
                                                      & (vlSelfRef.L1DCache__DOT__tag_5__DOT__rd_port_rdata_r 
                                                         >> 2U)) 
                                                     == 
                                                     (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                      >> 0xcU)) 
                                                    & (IData)(vlSelfRef.L1DCache__DOT__tag_5__DOT__rd_port_rdata_r))))) {
                                            if ((1U 
                                                 & (~ 
                                                    (((0xfffffffffffffULL 
                                                       & (vlSelfRef.L1DCache__DOT__tag_6__DOT__rd_port_rdata_r 
                                                          >> 2U)) 
                                                      == 
                                                      (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                       >> 0xcU)) 
                                                     & (IData)(vlSelfRef.L1DCache__DOT__tag_6__DOT__rd_port_rdata_r))))) {
                                                if (
                                                    (1U 
                                                     & (~ 
                                                        (((0xfffffffffffffULL 
                                                           & (vlSelfRef.L1DCache__DOT__tag_7__DOT__rd_port_rdata_r 
                                                              >> 2U)) 
                                                          == 
                                                          (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                           >> 0xcU)) 
                                                         & (IData)(vlSelfRef.L1DCache__DOT__tag_7__DOT__rd_port_rdata_r))))) {
                                                    vlSelfRef.L1DCache__DOT__data_rd_en_w = 0U;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        vlSelfRef.req_ready = L1DCache__DOT__req_ready_w;
        vlSelfRef.resp_valid = L1DCache__DOT__resp_valid_w;
    } else {
        vlSelfRef.req_ready = L1DCache__DOT__req_ready_w;
        vlSelfRef.resp_valid = L1DCache__DOT__resp_valid_w;
    }
    __Vtableidx1 = ((((IData)(vlSelfRef.L1DCache__DOT__fill_start_w) 
                      << 5U) | (((IData)(vlSelfRef.ar_ready) 
                                 << 4U) | ((IData)(vlSelfRef.r_last) 
                                           << 3U))) 
                    | (((IData)(vlSelfRef.r_valid) 
                        << 2U) | (IData)(vlSelfRef.L1DCache__DOT__fill_fsm__DOT__state_r)));
    vlSelfRef.L1DCache__DOT__fill_fsm__DOT__state_next 
        = VL1DCache__ConstPool__TABLE_h621fdd92_0[__Vtableidx1];
    __Vtableidx2 = ((((IData)(vlSelfRef.L1DCache__DOT__wb_start_w) 
                      << 9U) | ((IData)(vlSelfRef.aw_ready) 
                                << 8U)) | (((IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__beat_ctr_r) 
                                            << 4U) 
                                           | (((IData)(vlSelfRef.w_ready) 
                                               << 3U) 
                                              | (((IData)(vlSelfRef.b_valid) 
                                                  << 2U) 
                                                 | (IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__state_r)))));
    vlSelfRef.L1DCache__DOT__wb_fsm__DOT__state_next 
        = VL1DCache__ConstPool__TABLE_h49293a7b_0[__Vtableidx2];
    if ((1U & (~ ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r) 
                  >> 3U)))) {
        if ((4U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
            if ((2U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
                if ((1U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
                    if ((0U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                        vlSelfRef.L1DCache__DOT__data_rd_addr_w 
                            = ((0xfc0U & ((IData)((vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                   >> 6U)) 
                                          << 6U)) | 
                               ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r) 
                                << 3U));
                    } else if ((1U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                        vlSelfRef.L1DCache__DOT__data_rd_addr_w 
                            = (1U | ((0xfc0U & ((IData)(
                                                        (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                         >> 6U)) 
                                                << 6U)) 
                                     | ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r) 
                                        << 3U)));
                    } else if ((2U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                        vlSelfRef.L1DCache__DOT__data_rd_addr_w 
                            = (2U | ((0xfc0U & ((IData)(
                                                        (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                         >> 6U)) 
                                                << 6U)) 
                                     | ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r) 
                                        << 3U)));
                    } else if ((3U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                        vlSelfRef.L1DCache__DOT__data_rd_addr_w 
                            = (3U | ((0xfc0U & ((IData)(
                                                        (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                         >> 6U)) 
                                                << 6U)) 
                                     | ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r) 
                                        << 3U)));
                    } else if ((4U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                        vlSelfRef.L1DCache__DOT__data_rd_addr_w 
                            = (4U | ((0xfc0U & ((IData)(
                                                        (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                         >> 6U)) 
                                                << 6U)) 
                                     | ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r) 
                                        << 3U)));
                    } else if ((5U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                        vlSelfRef.L1DCache__DOT__data_rd_addr_w 
                            = (5U | ((0xfc0U & ((IData)(
                                                        (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                         >> 6U)) 
                                                << 6U)) 
                                     | ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r) 
                                        << 3U)));
                    } else if ((6U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                        vlSelfRef.L1DCache__DOT__data_rd_addr_w 
                            = (6U | ((0xfc0U & ((IData)(
                                                        (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                         >> 6U)) 
                                                << 6U)) 
                                     | ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r) 
                                        << 3U)));
                    } else if ((7U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                        vlSelfRef.L1DCache__DOT__data_rd_addr_w 
                            = (7U | ((0xfc0U & ((IData)(
                                                        (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                         >> 6U)) 
                                                << 6U)) 
                                     | ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r) 
                                        << 3U)));
                    }
                }
                if ((1U & (~ (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r)))) {
                    vlSelfRef.L1DCache__DOT__data_wr_data_w 
                        = vlSelfRef.L1DCache__DOT__ctrl__DOT__req_data_r;
                    L1DCache__DOT__resp_data_w = vlSelfRef.L1DCache__DOT__ctrl__DOT__req_data_r;
                }
            } else if ((1U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
                vlSelfRef.L1DCache__DOT__data_wr_data_w 
                    = vlSelfRef.L1DCache__DOT__fill_word_0_w;
                if ((0U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                    vlSelfRef.L1DCache__DOT__data_wr_data_w 
                        = vlSelfRef.L1DCache__DOT__fill_word_0_w;
                } else if ((1U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                    vlSelfRef.L1DCache__DOT__data_wr_data_w 
                        = vlSelfRef.L1DCache__DOT__fill_word_1_w;
                } else if ((2U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                    vlSelfRef.L1DCache__DOT__data_wr_data_w 
                        = vlSelfRef.L1DCache__DOT__fill_word_2_w;
                } else if ((3U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                    vlSelfRef.L1DCache__DOT__data_wr_data_w 
                        = vlSelfRef.L1DCache__DOT__fill_word_3_w;
                } else if ((4U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                    vlSelfRef.L1DCache__DOT__data_wr_data_w 
                        = vlSelfRef.L1DCache__DOT__fill_word_4_w;
                } else if ((5U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                    vlSelfRef.L1DCache__DOT__data_wr_data_w 
                        = vlSelfRef.L1DCache__DOT__fill_word_5_w;
                } else if ((6U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                    vlSelfRef.L1DCache__DOT__data_wr_data_w 
                        = vlSelfRef.L1DCache__DOT__fill_word_6_w;
                } else if ((7U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r))) {
                    vlSelfRef.L1DCache__DOT__data_wr_data_w 
                        = vlSelfRef.L1DCache__DOT__fill_word_7_w;
                }
                if (((7U == (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__beat_ctr_r)) 
                     & (~ (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__miss_is_store_r)))) {
                    L1DCache__DOT__resp_data_w = vlSelfRef.L1DCache__DOT__fill_word_0_w;
                    if ((1U == (7U & (IData)((vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                              >> 3U))))) {
                        L1DCache__DOT__resp_data_w 
                            = vlSelfRef.L1DCache__DOT__fill_word_1_w;
                    } else if ((2U == (7U & (IData)(
                                                    (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                     >> 3U))))) {
                        L1DCache__DOT__resp_data_w 
                            = vlSelfRef.L1DCache__DOT__fill_word_2_w;
                    } else if ((3U == (7U & (IData)(
                                                    (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                     >> 3U))))) {
                        L1DCache__DOT__resp_data_w 
                            = vlSelfRef.L1DCache__DOT__fill_word_3_w;
                    } else if ((4U == (7U & (IData)(
                                                    (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                     >> 3U))))) {
                        L1DCache__DOT__resp_data_w 
                            = vlSelfRef.L1DCache__DOT__fill_word_4_w;
                    } else if ((5U == (7U & (IData)(
                                                    (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                     >> 3U))))) {
                        L1DCache__DOT__resp_data_w 
                            = vlSelfRef.L1DCache__DOT__fill_word_5_w;
                    } else if ((6U == (7U & (IData)(
                                                    (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                     >> 3U))))) {
                        L1DCache__DOT__resp_data_w 
                            = vlSelfRef.L1DCache__DOT__fill_word_6_w;
                    } else if ((7U == (7U & (IData)(
                                                    (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                     >> 3U))))) {
                        L1DCache__DOT__resp_data_w 
                            = vlSelfRef.L1DCache__DOT__fill_word_7_w;
                    }
                }
            }
        } else {
            if ((1U & (~ ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r) 
                          >> 1U)))) {
                if ((1U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
                    vlSelfRef.L1DCache__DOT__data_rd_addr_w = 0U;
                    if ((((0xfffffffffffffULL & (vlSelfRef.L1DCache__DOT__tag_0__DOT__rd_port_rdata_r 
                                                 >> 2U)) 
                          == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                              >> 0xcU)) & (IData)(vlSelfRef.L1DCache__DOT__tag_0__DOT__rd_port_rdata_r))) {
                        vlSelfRef.L1DCache__DOT__data_rd_addr_w 
                            = ((0xfc0U & ((IData)((vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                   >> 6U)) 
                                          << 6U)) | 
                               (7U & (IData)((vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                              >> 3U))));
                    } else if ((((0xfffffffffffffULL 
                                  & (vlSelfRef.L1DCache__DOT__tag_1__DOT__rd_port_rdata_r 
                                     >> 2U)) == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                 >> 0xcU)) 
                                & (IData)(vlSelfRef.L1DCache__DOT__tag_1__DOT__rd_port_rdata_r))) {
                        vlSelfRef.L1DCache__DOT__data_rd_addr_w 
                            = (8U | ((0xfc0U & ((IData)(
                                                        (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                         >> 6U)) 
                                                << 6U)) 
                                     | (7U & (IData)(
                                                     (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                      >> 3U)))));
                    } else if ((((0xfffffffffffffULL 
                                  & (vlSelfRef.L1DCache__DOT__tag_2__DOT__rd_port_rdata_r 
                                     >> 2U)) == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                 >> 0xcU)) 
                                & (IData)(vlSelfRef.L1DCache__DOT__tag_2__DOT__rd_port_rdata_r))) {
                        vlSelfRef.L1DCache__DOT__data_rd_addr_w 
                            = (0x10U | ((0xfc0U & ((IData)(
                                                           (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                            >> 6U)) 
                                                   << 6U)) 
                                        | (7U & (IData)(
                                                        (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                         >> 3U)))));
                    } else if ((((0xfffffffffffffULL 
                                  & (vlSelfRef.L1DCache__DOT__tag_3__DOT__rd_port_rdata_r 
                                     >> 2U)) == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                 >> 0xcU)) 
                                & (IData)(vlSelfRef.L1DCache__DOT__tag_3__DOT__rd_port_rdata_r))) {
                        vlSelfRef.L1DCache__DOT__data_rd_addr_w 
                            = (0x18U | ((0xfc0U & ((IData)(
                                                           (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                            >> 6U)) 
                                                   << 6U)) 
                                        | (7U & (IData)(
                                                        (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                         >> 3U)))));
                    } else if ((((0xfffffffffffffULL 
                                  & (vlSelfRef.L1DCache__DOT__tag_4__DOT__rd_port_rdata_r 
                                     >> 2U)) == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                 >> 0xcU)) 
                                & (IData)(vlSelfRef.L1DCache__DOT__tag_4__DOT__rd_port_rdata_r))) {
                        vlSelfRef.L1DCache__DOT__data_rd_addr_w 
                            = (0x20U | ((0xfc0U & ((IData)(
                                                           (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                            >> 6U)) 
                                                   << 6U)) 
                                        | (7U & (IData)(
                                                        (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                         >> 3U)))));
                    } else if ((((0xfffffffffffffULL 
                                  & (vlSelfRef.L1DCache__DOT__tag_5__DOT__rd_port_rdata_r 
                                     >> 2U)) == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                 >> 0xcU)) 
                                & (IData)(vlSelfRef.L1DCache__DOT__tag_5__DOT__rd_port_rdata_r))) {
                        vlSelfRef.L1DCache__DOT__data_rd_addr_w 
                            = (0x28U | ((0xfc0U & ((IData)(
                                                           (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                            >> 6U)) 
                                                   << 6U)) 
                                        | (7U & (IData)(
                                                        (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                         >> 3U)))));
                    } else if ((((0xfffffffffffffULL 
                                  & (vlSelfRef.L1DCache__DOT__tag_6__DOT__rd_port_rdata_r 
                                     >> 2U)) == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                 >> 0xcU)) 
                                & (IData)(vlSelfRef.L1DCache__DOT__tag_6__DOT__rd_port_rdata_r))) {
                        vlSelfRef.L1DCache__DOT__data_rd_addr_w 
                            = (0x30U | ((0xfc0U & ((IData)(
                                                           (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                            >> 6U)) 
                                                   << 6U)) 
                                        | (7U & (IData)(
                                                        (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                         >> 3U)))));
                    } else if ((((0xfffffffffffffULL 
                                  & (vlSelfRef.L1DCache__DOT__tag_7__DOT__rd_port_rdata_r 
                                     >> 2U)) == (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                 >> 0xcU)) 
                                & (IData)(vlSelfRef.L1DCache__DOT__tag_7__DOT__rd_port_rdata_r))) {
                        vlSelfRef.L1DCache__DOT__data_rd_addr_w 
                            = (0x38U | ((0xfc0U & ((IData)(
                                                           (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                            >> 6U)) 
                                                   << 6U)) 
                                        | (7U & (IData)(
                                                        (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_addr_r 
                                                         >> 3U)))));
                    }
                }
            }
            if ((2U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
                if ((1U & (~ (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r)))) {
                    if (vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_hit_r) {
                        if (vlSelfRef.L1DCache__DOT__ctrl__DOT__req_is_store_r) {
                            vlSelfRef.L1DCache__DOT__data_wr_data_w 
                                = vlSelfRef.L1DCache__DOT__ctrl__DOT__req_data_r;
                        }
                    }
                    L1DCache__DOT__resp_data_w = vlSelfRef.L1DCache__DOT__data_ram__DOT__rd_port_rdata_r;
                }
            }
        }
        if ((1U & (~ ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r) 
                      >> 2U)))) {
            if ((2U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
                if ((1U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
                    L1DCache__DOT__lru_access_en_w = 1U;
                } else if (vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_hit_r) {
                    L1DCache__DOT__lru_access_en_w = 1U;
                }
            } else if ((1U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
                L1DCache__DOT__lru_access_en_w = 0U;
            }
        }
    }
    vlSelfRef.resp_data = L1DCache__DOT__resp_data_w;
    L1DCache__DOT__w_data_w = 0ULL;
    if ((2U & (IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__state_r))) {
        if ((1U & (~ (IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__state_r)))) {
            L1DCache__DOT__w_data_w = 0ULL;
            if ((0U == (IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__beat_ctr_r))) {
                L1DCache__DOT__w_data_w = L1DCache__DOT__wb_word_0_w;
            } else if ((1U == (IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__beat_ctr_r))) {
                L1DCache__DOT__w_data_w = L1DCache__DOT__wb_word_1_w;
            } else if ((2U == (IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__beat_ctr_r))) {
                L1DCache__DOT__w_data_w = L1DCache__DOT__wb_word_2_w;
            } else if ((3U == (IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__beat_ctr_r))) {
                L1DCache__DOT__w_data_w = L1DCache__DOT__wb_word_3_w;
            } else if ((4U == (IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__beat_ctr_r))) {
                L1DCache__DOT__w_data_w = L1DCache__DOT__wb_word_4_w;
            } else if ((5U == (IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__beat_ctr_r))) {
                L1DCache__DOT__w_data_w = L1DCache__DOT__wb_word_5_w;
            } else if ((6U == (IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__beat_ctr_r))) {
                L1DCache__DOT__w_data_w = L1DCache__DOT__wb_word_6_w;
            } else if ((7U == (IData)(vlSelfRef.L1DCache__DOT__wb_fsm__DOT__beat_ctr_r))) {
                L1DCache__DOT__w_data_w = L1DCache__DOT__wb_word_7_w;
            }
        }
    }
    if ((1U & (~ ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r) 
                  >> 3U)))) {
        if ((1U & (~ ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r) 
                      >> 2U)))) {
            if ((2U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
                if ((1U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
                    L1DCache__DOT__lru_access_way_w 
                        = vlSelfRef.L1DCache__DOT__ctrl__DOT__victim_way_r;
                    L1DCache__DOT__lru_tree_in_w = vlSelfRef.L1DCache__DOT__ctrl__DOT__lru_tree_r;
                } else if (vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_hit_r) {
                    L1DCache__DOT__lru_access_way_w 
                        = vlSelfRef.L1DCache__DOT__ctrl__DOT__hit_way_r;
                    L1DCache__DOT__lru_tree_in_w = vlSelfRef.L1DCache__DOT__ctrl__DOT__lru_tree_r;
                }
            } else if ((1U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
                L1DCache__DOT__lru_access_way_w = 0U;
                L1DCache__DOT__lru_tree_in_w = vlSelfRef.L1DCache__DOT__lru_ram__DOT__rd_port_rdata_r;
            }
        }
    }
    vlSelfRef.L1DCache__DOT__lru_upd__DOT__idx = ((1U 
                                                   & (IData)(L1DCache__DOT__lru_tree_in_w))
                                                   ? 0U
                                                   : 1U);
    vlSelfRef.L1DCache__DOT__lru_upd__DOT__idx = (7U 
                                                  & (((6U 
                                                       >= 
                                                       (7U 
                                                        & ((IData)(1U) 
                                                           + (IData)(vlSelfRef.L1DCache__DOT__lru_upd__DOT__idx)))) 
                                                      && (1U 
                                                          & ((IData)(L1DCache__DOT__lru_tree_in_w) 
                                                             >> 
                                                             (7U 
                                                              & ((IData)(1U) 
                                                                 + (IData)(vlSelfRef.L1DCache__DOT__lru_upd__DOT__idx))))))
                                                      ? 
                                                     VL_SHIFTL_III(3,32,32, (IData)(vlSelfRef.L1DCache__DOT__lru_upd__DOT__idx), 1U)
                                                      : 
                                                     (1U 
                                                      | VL_SHIFTL_III(3,32,32, (IData)(vlSelfRef.L1DCache__DOT__lru_upd__DOT__idx), 1U))));
    vlSelfRef.L1DCache__DOT__lru_upd__DOT__idx = (7U 
                                                  & (((6U 
                                                       >= 
                                                       (7U 
                                                        & ((IData)(3U) 
                                                           + (IData)(vlSelfRef.L1DCache__DOT__lru_upd__DOT__idx)))) 
                                                      && (1U 
                                                          & ((IData)(L1DCache__DOT__lru_tree_in_w) 
                                                             >> 
                                                             (7U 
                                                              & ((IData)(3U) 
                                                                 + (IData)(vlSelfRef.L1DCache__DOT__lru_upd__DOT__idx))))))
                                                      ? 
                                                     VL_SHIFTL_III(3,32,32, (IData)(vlSelfRef.L1DCache__DOT__lru_upd__DOT__idx), 1U)
                                                      : 
                                                     (1U 
                                                      | VL_SHIFTL_III(3,32,32, (IData)(vlSelfRef.L1DCache__DOT__lru_upd__DOT__idx), 1U))));
    L1DCache__DOT__lru_upd__DOT__updated = L1DCache__DOT__lru_tree_in_w;
    L1DCache__DOT__lru_upd__DOT__way_bit = (1U & ((IData)(L1DCache__DOT__lru_access_way_w) 
                                                  >> 2U));
    L1DCache__DOT__lru_upd__DOT____Vlvbound_h6f501444__0 
        = L1DCache__DOT__lru_upd__DOT__way_bit;
    L1DCache__DOT__lru_upd__DOT__updated = ((0x7eU 
                                             & (IData)(L1DCache__DOT__lru_upd__DOT__updated)) 
                                            | (IData)(L1DCache__DOT__lru_upd__DOT____Vlvbound_h6f501444__0));
    L1DCache__DOT__lru_upd__DOT__step = L1DCache__DOT__lru_upd__DOT__way_bit;
    L1DCache__DOT__lru_upd__DOT__way_bit = (1U & ((IData)(L1DCache__DOT__lru_access_way_w) 
                                                  >> 1U));
    L1DCache__DOT__lru_upd__DOT____Vlvbound_h6f501444__0 
        = L1DCache__DOT__lru_upd__DOT__way_bit;
    if ((6U >= (7U & ((IData)(1U) + (0x7fU & L1DCache__DOT__lru_upd__DOT__step))))) {
        L1DCache__DOT__lru_upd__DOT__updated = (((~ 
                                                  ((IData)(1U) 
                                                   << 
                                                   (7U 
                                                    & ((IData)(1U) 
                                                       + 
                                                       (0x7fU 
                                                        & L1DCache__DOT__lru_upd__DOT__step))))) 
                                                 & (IData)(L1DCache__DOT__lru_upd__DOT__updated)) 
                                                | (0x7fU 
                                                   & ((IData)(L1DCache__DOT__lru_upd__DOT____Vlvbound_h6f501444__0) 
                                                      << 
                                                      (7U 
                                                       & ((IData)(1U) 
                                                          + 
                                                          (0x7fU 
                                                           & L1DCache__DOT__lru_upd__DOT__step))))));
    }
    L1DCache__DOT__lru_upd__DOT__step = (VL_SHIFTL_III(32,32,32, L1DCache__DOT__lru_upd__DOT__step, 1U) 
                                         | (IData)(L1DCache__DOT__lru_upd__DOT__way_bit));
    L1DCache__DOT__lru_upd__DOT__way_bit = (1U & (IData)(L1DCache__DOT__lru_access_way_w));
    L1DCache__DOT__lru_upd__DOT____Vlvbound_h6f501444__0 
        = L1DCache__DOT__lru_upd__DOT__way_bit;
    if ((6U >= (7U & ((IData)(3U) + (0x7fU & L1DCache__DOT__lru_upd__DOT__step))))) {
        L1DCache__DOT__lru_upd__DOT__updated = (((~ 
                                                  ((IData)(1U) 
                                                   << 
                                                   (7U 
                                                    & ((IData)(3U) 
                                                       + 
                                                       (0x7fU 
                                                        & L1DCache__DOT__lru_upd__DOT__step))))) 
                                                 & (IData)(L1DCache__DOT__lru_upd__DOT__updated)) 
                                                | (0x7fU 
                                                   & ((IData)(L1DCache__DOT__lru_upd__DOT____Vlvbound_h6f501444__0) 
                                                      << 
                                                      (7U 
                                                       & ((IData)(3U) 
                                                          + 
                                                          (0x7fU 
                                                           & L1DCache__DOT__lru_upd__DOT__step))))));
    }
    L1DCache__DOT__lru_upd__DOT__step = (VL_SHIFTL_III(32,32,32, L1DCache__DOT__lru_upd__DOT__step, 1U) 
                                         | (IData)(L1DCache__DOT__lru_upd__DOT__way_bit));
    vlSelfRef.w_data = L1DCache__DOT__w_data_w;
    L1DCache__DOT__lru_tree_out_w = ((IData)(L1DCache__DOT__lru_access_en_w)
                                      ? (IData)(L1DCache__DOT__lru_upd__DOT__updated)
                                      : (IData)(L1DCache__DOT__lru_tree_in_w));
    vlSelfRef.L1DCache__DOT__lru_wr_data_w = 0U;
    if ((1U & (~ ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r) 
                  >> 3U)))) {
        if ((1U & (~ ((IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r) 
                      >> 2U)))) {
            if ((2U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
                if ((1U & (IData)(vlSelfRef.L1DCache__DOT__ctrl__DOT__state_r))) {
                    vlSelfRef.L1DCache__DOT__lru_wr_data_w 
                        = L1DCache__DOT__lru_tree_out_w;
                } else if (vlSelfRef.L1DCache__DOT__ctrl__DOT__lookup_hit_r) {
                    vlSelfRef.L1DCache__DOT__lru_wr_data_w 
                        = L1DCache__DOT__lru_tree_out_w;
                }
            }
        }
    }
}

void VL1DCache___024root___eval_triggers__act(VL1DCache___024root* vlSelf);

bool VL1DCache___024root___eval_phase__act(VL1DCache___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VL1DCache___024root___eval_phase__act\n"); );
    VL1DCache__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Init
    VlTriggerVec<1> __VpreTriggered;
    CData/*0:0*/ __VactExecute;
    // Body
    VL1DCache___024root___eval_triggers__act(vlSelf);
    __VactExecute = vlSelfRef.__VactTriggered.any();
    if (__VactExecute) {
        __VpreTriggered.andNot(vlSelfRef.__VactTriggered, vlSelfRef.__VnbaTriggered);
        vlSelfRef.__VnbaTriggered.thisOr(vlSelfRef.__VactTriggered);
        VL1DCache___024root___eval_act(vlSelf);
    }
    return (__VactExecute);
}

bool VL1DCache___024root___eval_phase__nba(VL1DCache___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VL1DCache___024root___eval_phase__nba\n"); );
    VL1DCache__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Init
    CData/*0:0*/ __VnbaExecute;
    // Body
    __VnbaExecute = vlSelfRef.__VnbaTriggered.any();
    if (__VnbaExecute) {
        VL1DCache___024root___eval_nba(vlSelf);
        vlSelfRef.__VnbaTriggered.clear();
    }
    return (__VnbaExecute);
}

#ifdef VL_DEBUG
VL_ATTR_COLD void VL1DCache___024root___dump_triggers__ico(VL1DCache___024root* vlSelf);
#endif  // VL_DEBUG
#ifdef VL_DEBUG
VL_ATTR_COLD void VL1DCache___024root___dump_triggers__nba(VL1DCache___024root* vlSelf);
#endif  // VL_DEBUG
#ifdef VL_DEBUG
VL_ATTR_COLD void VL1DCache___024root___dump_triggers__act(VL1DCache___024root* vlSelf);
#endif  // VL_DEBUG

void VL1DCache___024root___eval(VL1DCache___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VL1DCache___024root___eval\n"); );
    VL1DCache__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
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
            VL1DCache___024root___dump_triggers__ico(vlSelf);
#endif
            VL_FATAL_MT("tests/l1d/L1DCache.sv", 1126, "", "Input combinational region did not converge.");
        }
        __VicoIterCount = ((IData)(1U) + __VicoIterCount);
        __VicoContinue = 0U;
        if (VL1DCache___024root___eval_phase__ico(vlSelf)) {
            __VicoContinue = 1U;
        }
        vlSelfRef.__VicoFirstIteration = 0U;
    }
    __VnbaIterCount = 0U;
    __VnbaContinue = 1U;
    while (__VnbaContinue) {
        if (VL_UNLIKELY(((0x64U < __VnbaIterCount)))) {
#ifdef VL_DEBUG
            VL1DCache___024root___dump_triggers__nba(vlSelf);
#endif
            VL_FATAL_MT("tests/l1d/L1DCache.sv", 1126, "", "NBA region did not converge.");
        }
        __VnbaIterCount = ((IData)(1U) + __VnbaIterCount);
        __VnbaContinue = 0U;
        vlSelfRef.__VactIterCount = 0U;
        vlSelfRef.__VactContinue = 1U;
        while (vlSelfRef.__VactContinue) {
            if (VL_UNLIKELY(((0x64U < vlSelfRef.__VactIterCount)))) {
#ifdef VL_DEBUG
                VL1DCache___024root___dump_triggers__act(vlSelf);
#endif
                VL_FATAL_MT("tests/l1d/L1DCache.sv", 1126, "", "Active region did not converge.");
            }
            vlSelfRef.__VactIterCount = ((IData)(1U) 
                                         + vlSelfRef.__VactIterCount);
            vlSelfRef.__VactContinue = 0U;
            if (VL1DCache___024root___eval_phase__act(vlSelf)) {
                vlSelfRef.__VactContinue = 1U;
            }
        }
        if (VL1DCache___024root___eval_phase__nba(vlSelf)) {
            __VnbaContinue = 1U;
        }
    }
}

#ifdef VL_DEBUG
void VL1DCache___024root___eval_debug_assertions(VL1DCache___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VL1DCache___024root___eval_debug_assertions\n"); );
    VL1DCache__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    if (VL_UNLIKELY(((vlSelfRef.clk & 0xfeU)))) {
        Verilated::overWidthError("clk");}
    if (VL_UNLIKELY(((vlSelfRef.rst & 0xfeU)))) {
        Verilated::overWidthError("rst");}
    if (VL_UNLIKELY(((vlSelfRef.req_valid & 0xfeU)))) {
        Verilated::overWidthError("req_valid");}
    if (VL_UNLIKELY(((vlSelfRef.req_is_store & 0xfeU)))) {
        Verilated::overWidthError("req_is_store");}
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
    if (VL_UNLIKELY(((vlSelfRef.aw_ready & 0xfeU)))) {
        Verilated::overWidthError("aw_ready");}
    if (VL_UNLIKELY(((vlSelfRef.w_ready & 0xfeU)))) {
        Verilated::overWidthError("w_ready");}
    if (VL_UNLIKELY(((vlSelfRef.b_valid & 0xfeU)))) {
        Verilated::overWidthError("b_valid");}
    if (VL_UNLIKELY(((vlSelfRef.b_id & 0xf0U)))) {
        Verilated::overWidthError("b_id");}
    if (VL_UNLIKELY(((vlSelfRef.b_resp & 0xfcU)))) {
        Verilated::overWidthError("b_resp");}
}
#endif  // VL_DEBUG
