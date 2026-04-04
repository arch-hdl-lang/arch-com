// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Design implementation internals
// See VFsmCacheCtrl.h for the primary calling header

#include "VFsmCacheCtrl__pch.h"
#include "VFsmCacheCtrl___024root.h"

VL_ATTR_COLD void VFsmCacheCtrl___024root___eval_static(VFsmCacheCtrl___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmCacheCtrl___024root___eval_static\n"); );
    VFsmCacheCtrl__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
}

VL_ATTR_COLD void VFsmCacheCtrl___024root___eval_initial__TOP(VFsmCacheCtrl___024root* vlSelf);

VL_ATTR_COLD void VFsmCacheCtrl___024root___eval_initial(VFsmCacheCtrl___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmCacheCtrl___024root___eval_initial\n"); );
    VFsmCacheCtrl__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    VFsmCacheCtrl___024root___eval_initial__TOP(vlSelf);
    vlSelfRef.__Vtrigprevexpr___TOP__clk__0 = vlSelfRef.clk;
}

VL_ATTR_COLD void VFsmCacheCtrl___024root___eval_initial__TOP(VFsmCacheCtrl___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmCacheCtrl___024root___eval_initial__TOP\n"); );
    VFsmCacheCtrl__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    vlSelfRef.resp_error = 0U;
}

VL_ATTR_COLD void VFsmCacheCtrl___024root___eval_final(VFsmCacheCtrl___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmCacheCtrl___024root___eval_final\n"); );
    VFsmCacheCtrl__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
}

#ifdef VL_DEBUG
VL_ATTR_COLD void VFsmCacheCtrl___024root___dump_triggers__stl(VFsmCacheCtrl___024root* vlSelf);
#endif  // VL_DEBUG
VL_ATTR_COLD bool VFsmCacheCtrl___024root___eval_phase__stl(VFsmCacheCtrl___024root* vlSelf);

VL_ATTR_COLD void VFsmCacheCtrl___024root___eval_settle(VFsmCacheCtrl___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmCacheCtrl___024root___eval_settle\n"); );
    VFsmCacheCtrl__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
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
            VFsmCacheCtrl___024root___dump_triggers__stl(vlSelf);
#endif
            VL_FATAL_MT("tests/l1d/FsmCacheCtrl.sv", 6, "", "Settle region did not converge.");
        }
        __VstlIterCount = ((IData)(1U) + __VstlIterCount);
        __VstlContinue = 0U;
        if (VFsmCacheCtrl___024root___eval_phase__stl(vlSelf)) {
            __VstlContinue = 1U;
        }
        vlSelfRef.__VstlFirstIteration = 0U;
    }
}

#ifdef VL_DEBUG
VL_ATTR_COLD void VFsmCacheCtrl___024root___dump_triggers__stl(VFsmCacheCtrl___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmCacheCtrl___024root___dump_triggers__stl\n"); );
    VFsmCacheCtrl__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
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

VL_ATTR_COLD void VFsmCacheCtrl___024root___stl_sequent__TOP__0(VFsmCacheCtrl___024root* vlSelf);

VL_ATTR_COLD void VFsmCacheCtrl___024root___eval_stl(VFsmCacheCtrl___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmCacheCtrl___024root___eval_stl\n"); );
    VFsmCacheCtrl__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    if ((1ULL & vlSelfRef.__VstlTriggered.word(0U))) {
        VFsmCacheCtrl___024root___stl_sequent__TOP__0(vlSelf);
    }
}

VL_ATTR_COLD void VFsmCacheCtrl___024root___stl_sequent__TOP__0(VFsmCacheCtrl___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmCacheCtrl___024root___stl_sequent__TOP__0\n"); );
    VFsmCacheCtrl__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    vlSelfRef.req_ready = 0U;
    vlSelfRef.fill_start = 0U;
    vlSelfRef.wb_start = 0U;
    vlSelfRef.tag_rd_en_0 = 0U;
    vlSelfRef.tag_rd_en_1 = 0U;
    vlSelfRef.tag_rd_en_2 = 0U;
    vlSelfRef.tag_rd_en_3 = 0U;
    vlSelfRef.tag_rd_en_4 = 0U;
    vlSelfRef.tag_rd_en_5 = 0U;
    vlSelfRef.tag_rd_en_6 = 0U;
    vlSelfRef.tag_rd_en_7 = 0U;
    vlSelfRef.lru_rd_en = 0U;
    vlSelfRef.lru_wr_en = 0U;
    vlSelfRef.lru_access_en = 0U;
    vlSelfRef.fill_addr = 0ULL;
    vlSelfRef.wb_word_0 = 0ULL;
    vlSelfRef.wb_word_1 = 0ULL;
    vlSelfRef.wb_word_2 = 0ULL;
    vlSelfRef.wb_word_3 = 0ULL;
    vlSelfRef.wb_word_4 = 0ULL;
    vlSelfRef.wb_word_5 = 0ULL;
    vlSelfRef.wb_word_6 = 0ULL;
    vlSelfRef.wb_word_7 = 0ULL;
    vlSelfRef.tag_rd_addr_0 = 0U;
    vlSelfRef.tag_rd_addr_1 = 0U;
    vlSelfRef.tag_rd_addr_2 = 0U;
    vlSelfRef.tag_rd_addr_3 = 0U;
    vlSelfRef.tag_rd_addr_4 = 0U;
    vlSelfRef.tag_rd_addr_5 = 0U;
    vlSelfRef.tag_rd_addr_6 = 0U;
    vlSelfRef.data_wr_en = 0U;
    vlSelfRef.lru_rd_addr = 0U;
    vlSelfRef.lru_wr_addr = 0U;
    vlSelfRef.lru_wr_data = 0U;
    vlSelfRef.wb_addr = 0ULL;
    vlSelfRef.tag_rd_addr_7 = 0U;
    vlSelfRef.resp_valid = 0U;
    vlSelfRef.lru_tree_in = 0U;
    vlSelfRef.lru_access_way = 0U;
    vlSelfRef.tag_wr_en_0 = 0U;
    vlSelfRef.tag_wr_en_1 = 0U;
    vlSelfRef.tag_wr_en_2 = 0U;
    vlSelfRef.tag_wr_en_3 = 0U;
    vlSelfRef.tag_wr_en_4 = 0U;
    vlSelfRef.tag_wr_en_5 = 0U;
    vlSelfRef.tag_wr_en_6 = 0U;
    vlSelfRef.tag_wr_en_7 = 0U;
    vlSelfRef.tag_wr_addr_0 = 0U;
    vlSelfRef.tag_wr_data_0 = 0ULL;
    vlSelfRef.tag_wr_addr_1 = 0U;
    vlSelfRef.tag_wr_data_1 = 0ULL;
    vlSelfRef.tag_wr_addr_2 = 0U;
    vlSelfRef.tag_wr_data_2 = 0ULL;
    vlSelfRef.tag_wr_addr_3 = 0U;
    vlSelfRef.tag_wr_data_3 = 0ULL;
    vlSelfRef.tag_wr_addr_4 = 0U;
    vlSelfRef.tag_wr_data_4 = 0ULL;
    vlSelfRef.tag_wr_addr_5 = 0U;
    vlSelfRef.tag_wr_data_5 = 0ULL;
    vlSelfRef.tag_wr_addr_6 = 0U;
    vlSelfRef.tag_wr_data_6 = 0ULL;
    vlSelfRef.tag_wr_addr_7 = 0U;
    vlSelfRef.tag_wr_data_7 = 0ULL;
    vlSelfRef.data_wr_addr = 0U;
    vlSelfRef.FsmCacheCtrl__DOT__state_next = vlSelfRef.FsmCacheCtrl__DOT__state_r;
    if ((8U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
        if ((1U & (~ ((IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r) 
                      >> 2U)))) {
            if ((1U & (~ ((IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r) 
                          >> 1U)))) {
                if ((1U & (~ (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r)))) {
                    vlSelfRef.wb_start = 1U;
                    vlSelfRef.wb_word_0 = vlSelfRef.FsmCacheCtrl__DOT__wb_buf_0;
                    vlSelfRef.wb_word_1 = vlSelfRef.FsmCacheCtrl__DOT__wb_buf_1;
                    vlSelfRef.wb_word_2 = vlSelfRef.FsmCacheCtrl__DOT__wb_buf_2;
                    vlSelfRef.wb_word_3 = vlSelfRef.FsmCacheCtrl__DOT__wb_buf_3;
                    vlSelfRef.wb_word_4 = vlSelfRef.FsmCacheCtrl__DOT__wb_buf_4;
                    vlSelfRef.wb_word_5 = vlSelfRef.FsmCacheCtrl__DOT__wb_buf_5;
                    vlSelfRef.wb_word_6 = vlSelfRef.FsmCacheCtrl__DOT__wb_buf_6;
                    vlSelfRef.wb_word_7 = vlSelfRef.FsmCacheCtrl__DOT__wb_buf_7;
                    vlSelfRef.wb_addr = (VL_SHIFTL_QQI(64,64,32, vlSelfRef.FsmCacheCtrl__DOT__victim_tag_r, 0xcU) 
                                         | VL_SHIFTL_QQI(64,64,32, (QData)((IData)(
                                                                                (0x3fU 
                                                                                & (IData)(
                                                                                (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                                >> 6U))))), 6U));
                }
            }
        }
        if ((4U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
            vlSelfRef.FsmCacheCtrl__DOT__state_next 
                = vlSelfRef.FsmCacheCtrl__DOT__state_r;
        } else if ((2U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
            vlSelfRef.FsmCacheCtrl__DOT__state_next 
                = vlSelfRef.FsmCacheCtrl__DOT__state_r;
        } else if ((1U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
            vlSelfRef.FsmCacheCtrl__DOT__state_next 
                = vlSelfRef.FsmCacheCtrl__DOT__state_r;
        } else if (vlSelfRef.wb_done) {
            vlSelfRef.FsmCacheCtrl__DOT__state_next = 3U;
        }
    } else if ((4U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
        if ((2U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
            if ((1U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
                if ((8U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                    vlSelfRef.FsmCacheCtrl__DOT__state_next = 8U;
                }
            } else {
                vlSelfRef.FsmCacheCtrl__DOT__state_next = 0U;
            }
        } else if ((1U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
            if (((7U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r)) 
                 & (~ (IData)(vlSelfRef.FsmCacheCtrl__DOT__miss_is_store_r)))) {
                vlSelfRef.FsmCacheCtrl__DOT__state_next = 0U;
            } else if (((7U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r)) 
                        & (IData)(vlSelfRef.FsmCacheCtrl__DOT__miss_is_store_r))) {
                vlSelfRef.FsmCacheCtrl__DOT__state_next = 6U;
            }
        } else if (vlSelfRef.fill_done) {
            vlSelfRef.FsmCacheCtrl__DOT__state_next = 5U;
        }
    } else if ((2U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
        if ((1U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
            vlSelfRef.FsmCacheCtrl__DOT__state_next = 4U;
        } else if (vlSelfRef.FsmCacheCtrl__DOT__lookup_hit_r) {
            vlSelfRef.FsmCacheCtrl__DOT__state_next = 0U;
        } else if (((~ (IData)(vlSelfRef.FsmCacheCtrl__DOT__lookup_hit_r)) 
                    & (IData)(vlSelfRef.FsmCacheCtrl__DOT__lookup_victim_dirty_r))) {
            vlSelfRef.FsmCacheCtrl__DOT__state_next = 7U;
        } else if ((1U & ((~ (IData)(vlSelfRef.FsmCacheCtrl__DOT__lookup_hit_r)) 
                          & (~ (IData)(vlSelfRef.FsmCacheCtrl__DOT__lookup_victim_dirty_r))))) {
            vlSelfRef.FsmCacheCtrl__DOT__state_next = 3U;
        }
    } else if ((1U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
        vlSelfRef.FsmCacheCtrl__DOT__state_next = 2U;
    } else if (vlSelfRef.req_valid) {
        vlSelfRef.FsmCacheCtrl__DOT__state_next = 1U;
    }
    vlSelfRef.data_rd_en = 0U;
    vlSelfRef.data_rd_addr = 0U;
    vlSelfRef.data_wr_data = 0ULL;
    vlSelfRef.resp_data = 0ULL;
    if ((1U & (~ ((IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r) 
                  >> 3U)))) {
        if ((1U & (~ ((IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r) 
                      >> 2U)))) {
            if ((1U & (~ ((IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r) 
                          >> 1U)))) {
                if ((1U & (~ (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r)))) {
                    vlSelfRef.req_ready = 1U;
                    if (vlSelfRef.req_valid) {
                        vlSelfRef.tag_rd_en_0 = 1U;
                        vlSelfRef.tag_rd_en_1 = 1U;
                        vlSelfRef.tag_rd_en_2 = 1U;
                        vlSelfRef.tag_rd_en_3 = 1U;
                        vlSelfRef.tag_rd_en_4 = 1U;
                        vlSelfRef.tag_rd_en_5 = 1U;
                        vlSelfRef.tag_rd_en_6 = 1U;
                        vlSelfRef.tag_rd_en_7 = 1U;
                        vlSelfRef.lru_rd_en = 1U;
                        vlSelfRef.tag_rd_addr_0 = (0x3fU 
                                                   & (IData)(
                                                             (vlSelfRef.req_vaddr 
                                                              >> 6U)));
                        vlSelfRef.tag_rd_addr_1 = (0x3fU 
                                                   & (IData)(
                                                             (vlSelfRef.req_vaddr 
                                                              >> 6U)));
                        vlSelfRef.tag_rd_addr_2 = (0x3fU 
                                                   & (IData)(
                                                             (vlSelfRef.req_vaddr 
                                                              >> 6U)));
                        vlSelfRef.tag_rd_addr_3 = (0x3fU 
                                                   & (IData)(
                                                             (vlSelfRef.req_vaddr 
                                                              >> 6U)));
                        vlSelfRef.tag_rd_addr_4 = (0x3fU 
                                                   & (IData)(
                                                             (vlSelfRef.req_vaddr 
                                                              >> 6U)));
                        vlSelfRef.tag_rd_addr_5 = (0x3fU 
                                                   & (IData)(
                                                             (vlSelfRef.req_vaddr 
                                                              >> 6U)));
                        vlSelfRef.tag_rd_addr_6 = (0x3fU 
                                                   & (IData)(
                                                             (vlSelfRef.req_vaddr 
                                                              >> 6U)));
                        vlSelfRef.lru_rd_addr = (0x3fU 
                                                 & (IData)(
                                                           (vlSelfRef.req_vaddr 
                                                            >> 6U)));
                        vlSelfRef.tag_rd_addr_7 = (0x3fU 
                                                   & (IData)(
                                                             (vlSelfRef.req_vaddr 
                                                              >> 6U)));
                    }
                }
            }
            if ((2U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
                if ((1U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
                    vlSelfRef.fill_start = 1U;
                    vlSelfRef.lru_wr_en = 1U;
                    vlSelfRef.lru_access_en = 1U;
                    vlSelfRef.fill_addr = vlSelfRef.FsmCacheCtrl__DOT__req_addr_r;
                    vlSelfRef.lru_wr_addr = (0x3fU 
                                             & (IData)(
                                                       (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                        >> 6U)));
                    vlSelfRef.lru_wr_data = vlSelfRef.lru_tree_out;
                    vlSelfRef.lru_tree_in = vlSelfRef.FsmCacheCtrl__DOT__lru_tree_r;
                    vlSelfRef.lru_access_way = vlSelfRef.FsmCacheCtrl__DOT__victim_way_r;
                } else if (vlSelfRef.FsmCacheCtrl__DOT__lookup_hit_r) {
                    vlSelfRef.lru_wr_en = 1U;
                    vlSelfRef.lru_access_en = 1U;
                    vlSelfRef.lru_wr_addr = (0x3fU 
                                             & (IData)(
                                                       (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                        >> 6U)));
                    vlSelfRef.lru_wr_data = vlSelfRef.lru_tree_out;
                    vlSelfRef.lru_tree_in = vlSelfRef.FsmCacheCtrl__DOT__lru_tree_r;
                    vlSelfRef.lru_access_way = vlSelfRef.FsmCacheCtrl__DOT__hit_way_r;
                }
            } else if ((1U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
                vlSelfRef.lru_access_en = 0U;
                vlSelfRef.lru_tree_in = vlSelfRef.lru_rd_data;
                vlSelfRef.lru_access_way = 0U;
            }
        }
        if ((4U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
            if ((2U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
                if ((1U & (~ (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r)))) {
                    vlSelfRef.data_wr_en = 1U;
                    vlSelfRef.resp_valid = 1U;
                    if ((0U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                        vlSelfRef.tag_wr_en_0 = 1U;
                        vlSelfRef.tag_wr_addr_0 = (0x3fU 
                                                   & (IData)(
                                                             (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                              >> 6U)));
                        vlSelfRef.tag_wr_data_0 = (3ULL 
                                                   | (0x3ffffffffffffcULL 
                                                      & (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                         >> 0xaU)));
                    }
                    if ((0U != (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                        if ((1U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                            vlSelfRef.tag_wr_en_1 = 1U;
                            vlSelfRef.tag_wr_addr_1 
                                = (0x3fU & (IData)(
                                                   (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                    >> 6U)));
                            vlSelfRef.tag_wr_data_1 
                                = (3ULL | (0x3ffffffffffffcULL 
                                           & (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                              >> 0xaU)));
                        }
                        if ((1U != (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                            if ((2U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                                vlSelfRef.tag_wr_en_2 = 1U;
                                vlSelfRef.tag_wr_addr_2 
                                    = (0x3fU & (IData)(
                                                       (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                        >> 6U)));
                                vlSelfRef.tag_wr_data_2 
                                    = (3ULL | (0x3ffffffffffffcULL 
                                               & (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                  >> 0xaU)));
                            }
                            if ((2U != (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                                if ((3U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                                    vlSelfRef.tag_wr_en_3 = 1U;
                                    vlSelfRef.tag_wr_addr_3 
                                        = (0x3fU & (IData)(
                                                           (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                            >> 6U)));
                                    vlSelfRef.tag_wr_data_3 
                                        = (3ULL | (0x3ffffffffffffcULL 
                                                   & (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                      >> 0xaU)));
                                }
                                if ((3U != (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                                    if ((4U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                                        vlSelfRef.tag_wr_en_4 = 1U;
                                        vlSelfRef.tag_wr_addr_4 
                                            = (0x3fU 
                                               & (IData)(
                                                         (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                          >> 6U)));
                                        vlSelfRef.tag_wr_data_4 
                                            = (3ULL 
                                               | (0x3ffffffffffffcULL 
                                                  & (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                     >> 0xaU)));
                                    }
                                    if ((4U != (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                                        if ((5U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                                            vlSelfRef.tag_wr_en_5 = 1U;
                                            vlSelfRef.tag_wr_addr_5 
                                                = (0x3fU 
                                                   & (IData)(
                                                             (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                              >> 6U)));
                                            vlSelfRef.tag_wr_data_5 
                                                = (3ULL 
                                                   | (0x3ffffffffffffcULL 
                                                      & (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                         >> 0xaU)));
                                        }
                                        if ((5U != (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                                            if ((6U 
                                                 == (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                                                vlSelfRef.tag_wr_en_6 = 1U;
                                                vlSelfRef.tag_wr_addr_6 
                                                    = 
                                                    (0x3fU 
                                                     & (IData)(
                                                               (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                >> 6U)));
                                                vlSelfRef.tag_wr_data_6 
                                                    = 
                                                    (3ULL 
                                                     | (0x3ffffffffffffcULL 
                                                        & (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                           >> 0xaU)));
                                            }
                                            if ((6U 
                                                 != (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                                                if (
                                                    (7U 
                                                     == (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                                                    vlSelfRef.tag_wr_en_7 = 1U;
                                                    vlSelfRef.tag_wr_addr_7 
                                                        = 
                                                        (0x3fU 
                                                         & (IData)(
                                                                   (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                    >> 6U)));
                                                    vlSelfRef.tag_wr_data_7 
                                                        = 
                                                        (3ULL 
                                                         | (0x3ffffffffffffcULL 
                                                            & (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                               >> 0xaU)));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    vlSelfRef.data_wr_addr = ((0xfc0U 
                                               & ((IData)(
                                                          (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                           >> 6U)) 
                                                  << 6U)) 
                                              | (((IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r) 
                                                  << 3U) 
                                                 | (7U 
                                                    & (IData)(
                                                              (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                               >> 3U)))));
                    vlSelfRef.data_wr_data = vlSelfRef.FsmCacheCtrl__DOT__req_data_r;
                    vlSelfRef.resp_data = vlSelfRef.FsmCacheCtrl__DOT__req_data_r;
                }
                if ((1U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
                    if ((0U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                        vlSelfRef.data_rd_en = 1U;
                        vlSelfRef.data_rd_addr = ((0xfc0U 
                                                   & ((IData)(
                                                              (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                               >> 6U)) 
                                                      << 6U)) 
                                                  | ((IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r) 
                                                     << 3U));
                    } else if ((1U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                        vlSelfRef.data_rd_en = 1U;
                        vlSelfRef.data_rd_addr = (1U 
                                                  | ((0xfc0U 
                                                      & ((IData)(
                                                                 (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                  >> 6U)) 
                                                         << 6U)) 
                                                     | ((IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r) 
                                                        << 3U)));
                    } else if ((2U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                        vlSelfRef.data_rd_en = 1U;
                        vlSelfRef.data_rd_addr = (2U 
                                                  | ((0xfc0U 
                                                      & ((IData)(
                                                                 (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                  >> 6U)) 
                                                         << 6U)) 
                                                     | ((IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r) 
                                                        << 3U)));
                    } else if ((3U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                        vlSelfRef.data_rd_en = 1U;
                        vlSelfRef.data_rd_addr = (3U 
                                                  | ((0xfc0U 
                                                      & ((IData)(
                                                                 (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                  >> 6U)) 
                                                         << 6U)) 
                                                     | ((IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r) 
                                                        << 3U)));
                    } else if ((4U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                        vlSelfRef.data_rd_en = 1U;
                        vlSelfRef.data_rd_addr = (4U 
                                                  | ((0xfc0U 
                                                      & ((IData)(
                                                                 (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                  >> 6U)) 
                                                         << 6U)) 
                                                     | ((IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r) 
                                                        << 3U)));
                    } else if ((5U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                        vlSelfRef.data_rd_en = 1U;
                        vlSelfRef.data_rd_addr = (5U 
                                                  | ((0xfc0U 
                                                      & ((IData)(
                                                                 (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                  >> 6U)) 
                                                         << 6U)) 
                                                     | ((IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r) 
                                                        << 3U)));
                    } else if ((6U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                        vlSelfRef.data_rd_en = 1U;
                        vlSelfRef.data_rd_addr = (6U 
                                                  | ((0xfc0U 
                                                      & ((IData)(
                                                                 (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                  >> 6U)) 
                                                         << 6U)) 
                                                     | ((IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r) 
                                                        << 3U)));
                    } else if ((7U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                        vlSelfRef.data_rd_en = 1U;
                        vlSelfRef.data_rd_addr = (7U 
                                                  | ((0xfc0U 
                                                      & ((IData)(
                                                                 (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                  >> 6U)) 
                                                         << 6U)) 
                                                     | ((IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r) 
                                                        << 3U)));
                    }
                }
            } else if ((1U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
                vlSelfRef.data_wr_en = 1U;
                if (((7U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r)) 
                     & (~ (IData)(vlSelfRef.FsmCacheCtrl__DOT__miss_is_store_r)))) {
                    vlSelfRef.resp_valid = 1U;
                    vlSelfRef.resp_data = vlSelfRef.fill_word_0;
                    if ((1U == (7U & (IData)((vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                              >> 3U))))) {
                        vlSelfRef.resp_data = vlSelfRef.fill_word_1;
                    } else if ((2U == (7U & (IData)(
                                                    (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                     >> 3U))))) {
                        vlSelfRef.resp_data = vlSelfRef.fill_word_2;
                    } else if ((3U == (7U & (IData)(
                                                    (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                     >> 3U))))) {
                        vlSelfRef.resp_data = vlSelfRef.fill_word_3;
                    } else if ((4U == (7U & (IData)(
                                                    (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                     >> 3U))))) {
                        vlSelfRef.resp_data = vlSelfRef.fill_word_4;
                    } else if ((5U == (7U & (IData)(
                                                    (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                     >> 3U))))) {
                        vlSelfRef.resp_data = vlSelfRef.fill_word_5;
                    } else if ((6U == (7U & (IData)(
                                                    (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                     >> 3U))))) {
                        vlSelfRef.resp_data = vlSelfRef.fill_word_6;
                    } else if ((7U == (7U & (IData)(
                                                    (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                     >> 3U))))) {
                        vlSelfRef.resp_data = vlSelfRef.fill_word_7;
                    }
                }
                vlSelfRef.data_wr_addr = ((0xfc0U & 
                                           ((IData)(
                                                    (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                     >> 6U)) 
                                            << 6U)) 
                                          | ((IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r) 
                                             << 3U));
                vlSelfRef.data_wr_data = vlSelfRef.fill_word_0;
                if ((0U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                    vlSelfRef.data_wr_addr = ((0xfc0U 
                                               & ((IData)(
                                                          (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                           >> 6U)) 
                                                  << 6U)) 
                                              | ((IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r) 
                                                 << 3U));
                    vlSelfRef.data_wr_data = vlSelfRef.fill_word_0;
                } else if ((1U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                    vlSelfRef.data_wr_addr = (1U | 
                                              ((0xfc0U 
                                                & ((IData)(
                                                           (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                            >> 6U)) 
                                                   << 6U)) 
                                               | ((IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r) 
                                                  << 3U)));
                    vlSelfRef.data_wr_data = vlSelfRef.fill_word_1;
                } else if ((2U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                    vlSelfRef.data_wr_addr = (2U | 
                                              ((0xfc0U 
                                                & ((IData)(
                                                           (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                            >> 6U)) 
                                                   << 6U)) 
                                               | ((IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r) 
                                                  << 3U)));
                    vlSelfRef.data_wr_data = vlSelfRef.fill_word_2;
                } else if ((3U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                    vlSelfRef.data_wr_addr = (3U | 
                                              ((0xfc0U 
                                                & ((IData)(
                                                           (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                            >> 6U)) 
                                                   << 6U)) 
                                               | ((IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r) 
                                                  << 3U)));
                    vlSelfRef.data_wr_data = vlSelfRef.fill_word_3;
                } else if ((4U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                    vlSelfRef.data_wr_addr = (4U | 
                                              ((0xfc0U 
                                                & ((IData)(
                                                           (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                            >> 6U)) 
                                                   << 6U)) 
                                               | ((IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r) 
                                                  << 3U)));
                    vlSelfRef.data_wr_data = vlSelfRef.fill_word_4;
                } else if ((5U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                    vlSelfRef.data_wr_addr = (5U | 
                                              ((0xfc0U 
                                                & ((IData)(
                                                           (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                            >> 6U)) 
                                                   << 6U)) 
                                               | ((IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r) 
                                                  << 3U)));
                    vlSelfRef.data_wr_data = vlSelfRef.fill_word_5;
                } else if ((6U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                    vlSelfRef.data_wr_addr = (6U | 
                                              ((0xfc0U 
                                                & ((IData)(
                                                           (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                            >> 6U)) 
                                                   << 6U)) 
                                               | ((IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r) 
                                                  << 3U)));
                    vlSelfRef.data_wr_data = vlSelfRef.fill_word_6;
                } else if ((7U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                    vlSelfRef.data_wr_addr = (7U | 
                                              ((0xfc0U 
                                                & ((IData)(
                                                           (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                            >> 6U)) 
                                                   << 6U)) 
                                               | ((IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r) 
                                                  << 3U)));
                    vlSelfRef.data_wr_data = vlSelfRef.fill_word_7;
                }
            }
        } else {
            if ((2U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
                if ((1U & (~ (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r)))) {
                    if (vlSelfRef.FsmCacheCtrl__DOT__lookup_hit_r) {
                        if (vlSelfRef.FsmCacheCtrl__DOT__req_is_store_r) {
                            vlSelfRef.data_wr_en = 1U;
                            vlSelfRef.data_wr_addr 
                                = ((0xfc0U & ((IData)(
                                                      (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                       >> 6U)) 
                                              << 6U)) 
                                   | (((IData)(vlSelfRef.FsmCacheCtrl__DOT__hit_way_r) 
                                       << 3U) | (7U 
                                                 & (IData)(
                                                           (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                            >> 3U)))));
                            vlSelfRef.data_wr_data 
                                = vlSelfRef.FsmCacheCtrl__DOT__req_data_r;
                        }
                        vlSelfRef.resp_valid = 1U;
                    } else {
                        vlSelfRef.resp_valid = 0U;
                    }
                    vlSelfRef.resp_data = vlSelfRef.data_rd_data;
                }
                if ((1U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
                    if ((0U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                        vlSelfRef.tag_wr_en_0 = 1U;
                        vlSelfRef.tag_wr_addr_0 = (0x3fU 
                                                   & (IData)(
                                                             (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                              >> 6U)));
                        vlSelfRef.tag_wr_data_0 = (1ULL 
                                                   | (0x3ffffffffffffcULL 
                                                      & (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                         >> 0xaU)));
                    }
                    if ((0U != (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                        if ((1U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                            vlSelfRef.tag_wr_en_1 = 1U;
                            vlSelfRef.tag_wr_addr_1 
                                = (0x3fU & (IData)(
                                                   (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                    >> 6U)));
                            vlSelfRef.tag_wr_data_1 
                                = (1ULL | (0x3ffffffffffffcULL 
                                           & (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                              >> 0xaU)));
                        }
                        if ((1U != (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                            if ((2U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                                vlSelfRef.tag_wr_en_2 = 1U;
                                vlSelfRef.tag_wr_addr_2 
                                    = (0x3fU & (IData)(
                                                       (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                        >> 6U)));
                                vlSelfRef.tag_wr_data_2 
                                    = (1ULL | (0x3ffffffffffffcULL 
                                               & (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                  >> 0xaU)));
                            }
                            if ((2U != (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                                if ((3U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                                    vlSelfRef.tag_wr_en_3 = 1U;
                                    vlSelfRef.tag_wr_addr_3 
                                        = (0x3fU & (IData)(
                                                           (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                            >> 6U)));
                                    vlSelfRef.tag_wr_data_3 
                                        = (1ULL | (0x3ffffffffffffcULL 
                                                   & (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                      >> 0xaU)));
                                }
                                if ((3U != (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                                    if ((4U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                                        vlSelfRef.tag_wr_en_4 = 1U;
                                        vlSelfRef.tag_wr_addr_4 
                                            = (0x3fU 
                                               & (IData)(
                                                         (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                          >> 6U)));
                                        vlSelfRef.tag_wr_data_4 
                                            = (1ULL 
                                               | (0x3ffffffffffffcULL 
                                                  & (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                     >> 0xaU)));
                                    }
                                    if ((4U != (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                                        if ((5U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                                            vlSelfRef.tag_wr_en_5 = 1U;
                                            vlSelfRef.tag_wr_addr_5 
                                                = (0x3fU 
                                                   & (IData)(
                                                             (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                              >> 6U)));
                                            vlSelfRef.tag_wr_data_5 
                                                = (1ULL 
                                                   | (0x3ffffffffffffcULL 
                                                      & (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                         >> 0xaU)));
                                        }
                                        if ((5U != (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                                            if ((6U 
                                                 == (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                                                vlSelfRef.tag_wr_en_6 = 1U;
                                                vlSelfRef.tag_wr_addr_6 
                                                    = 
                                                    (0x3fU 
                                                     & (IData)(
                                                               (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                >> 6U)));
                                                vlSelfRef.tag_wr_data_6 
                                                    = 
                                                    (1ULL 
                                                     | (0x3ffffffffffffcULL 
                                                        & (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                           >> 0xaU)));
                                            }
                                            if ((6U 
                                                 != (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                                                if (
                                                    (7U 
                                                     == (IData)(vlSelfRef.FsmCacheCtrl__DOT__victim_way_r))) {
                                                    vlSelfRef.tag_wr_en_7 = 1U;
                                                    vlSelfRef.tag_wr_addr_7 
                                                        = 
                                                        (0x3fU 
                                                         & (IData)(
                                                                   (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                    >> 6U)));
                                                    vlSelfRef.tag_wr_data_7 
                                                        = 
                                                        (1ULL 
                                                         | (0x3ffffffffffffcULL 
                                                            & (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                               >> 0xaU)));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else if (vlSelfRef.FsmCacheCtrl__DOT__lookup_hit_r) {
                    if (vlSelfRef.FsmCacheCtrl__DOT__req_is_store_r) {
                        if ((0U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__hit_way_r))) {
                            vlSelfRef.tag_wr_en_0 = 1U;
                            vlSelfRef.tag_wr_addr_0 
                                = (0x3fU & (IData)(
                                                   (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                    >> 6U)));
                            vlSelfRef.tag_wr_data_0 
                                = (3ULL | (0x3ffffffffffffcULL 
                                           & (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                              >> 0xaU)));
                        }
                        if ((0U != (IData)(vlSelfRef.FsmCacheCtrl__DOT__hit_way_r))) {
                            if ((1U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__hit_way_r))) {
                                vlSelfRef.tag_wr_en_1 = 1U;
                                vlSelfRef.tag_wr_addr_1 
                                    = (0x3fU & (IData)(
                                                       (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                        >> 6U)));
                                vlSelfRef.tag_wr_data_1 
                                    = (3ULL | (0x3ffffffffffffcULL 
                                               & (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                  >> 0xaU)));
                            }
                            if ((1U != (IData)(vlSelfRef.FsmCacheCtrl__DOT__hit_way_r))) {
                                if ((2U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__hit_way_r))) {
                                    vlSelfRef.tag_wr_en_2 = 1U;
                                    vlSelfRef.tag_wr_addr_2 
                                        = (0x3fU & (IData)(
                                                           (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                            >> 6U)));
                                    vlSelfRef.tag_wr_data_2 
                                        = (3ULL | (0x3ffffffffffffcULL 
                                                   & (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                      >> 0xaU)));
                                }
                                if ((2U != (IData)(vlSelfRef.FsmCacheCtrl__DOT__hit_way_r))) {
                                    if ((3U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__hit_way_r))) {
                                        vlSelfRef.tag_wr_en_3 = 1U;
                                        vlSelfRef.tag_wr_addr_3 
                                            = (0x3fU 
                                               & (IData)(
                                                         (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                          >> 6U)));
                                        vlSelfRef.tag_wr_data_3 
                                            = (3ULL 
                                               | (0x3ffffffffffffcULL 
                                                  & (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                     >> 0xaU)));
                                    }
                                    if ((3U != (IData)(vlSelfRef.FsmCacheCtrl__DOT__hit_way_r))) {
                                        if ((4U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__hit_way_r))) {
                                            vlSelfRef.tag_wr_en_4 = 1U;
                                            vlSelfRef.tag_wr_addr_4 
                                                = (0x3fU 
                                                   & (IData)(
                                                             (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                              >> 6U)));
                                            vlSelfRef.tag_wr_data_4 
                                                = (3ULL 
                                                   | (0x3ffffffffffffcULL 
                                                      & (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                         >> 0xaU)));
                                        }
                                        if ((4U != (IData)(vlSelfRef.FsmCacheCtrl__DOT__hit_way_r))) {
                                            if ((5U 
                                                 == (IData)(vlSelfRef.FsmCacheCtrl__DOT__hit_way_r))) {
                                                vlSelfRef.tag_wr_en_5 = 1U;
                                                vlSelfRef.tag_wr_addr_5 
                                                    = 
                                                    (0x3fU 
                                                     & (IData)(
                                                               (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                >> 6U)));
                                                vlSelfRef.tag_wr_data_5 
                                                    = 
                                                    (3ULL 
                                                     | (0x3ffffffffffffcULL 
                                                        & (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                           >> 0xaU)));
                                            }
                                            if ((5U 
                                                 != (IData)(vlSelfRef.FsmCacheCtrl__DOT__hit_way_r))) {
                                                if (
                                                    (6U 
                                                     == (IData)(vlSelfRef.FsmCacheCtrl__DOT__hit_way_r))) {
                                                    vlSelfRef.tag_wr_en_6 = 1U;
                                                    vlSelfRef.tag_wr_addr_6 
                                                        = 
                                                        (0x3fU 
                                                         & (IData)(
                                                                   (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                    >> 6U)));
                                                    vlSelfRef.tag_wr_data_6 
                                                        = 
                                                        (3ULL 
                                                         | (0x3ffffffffffffcULL 
                                                            & (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                               >> 0xaU)));
                                                }
                                                if (
                                                    (6U 
                                                     != (IData)(vlSelfRef.FsmCacheCtrl__DOT__hit_way_r))) {
                                                    if (
                                                        (7U 
                                                         == (IData)(vlSelfRef.FsmCacheCtrl__DOT__hit_way_r))) {
                                                        vlSelfRef.tag_wr_en_7 = 1U;
                                                        vlSelfRef.tag_wr_addr_7 
                                                            = 
                                                            (0x3fU 
                                                             & (IData)(
                                                                       (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                        >> 6U)));
                                                        vlSelfRef.tag_wr_data_7 
                                                            = 
                                                            (3ULL 
                                                             | (0x3ffffffffffffcULL 
                                                                & (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
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
            if ((1U & (~ ((IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r) 
                          >> 1U)))) {
                if ((1U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
                    vlSelfRef.data_rd_en = 1U;
                    if ((1U & (~ (((0xfffffffffffffULL 
                                    & (vlSelfRef.tag_rd_data_0 
                                       >> 2U)) == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                   >> 0xcU)) 
                                  & (IData)(vlSelfRef.tag_rd_data_0))))) {
                        if ((1U & (~ (((0xfffffffffffffULL 
                                        & (vlSelfRef.tag_rd_data_1 
                                           >> 2U)) 
                                       == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                           >> 0xcU)) 
                                      & (IData)(vlSelfRef.tag_rd_data_1))))) {
                            if ((1U & (~ (((0xfffffffffffffULL 
                                            & (vlSelfRef.tag_rd_data_2 
                                               >> 2U)) 
                                           == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                               >> 0xcU)) 
                                          & (IData)(vlSelfRef.tag_rd_data_2))))) {
                                if ((1U & (~ (((0xfffffffffffffULL 
                                                & (vlSelfRef.tag_rd_data_3 
                                                   >> 2U)) 
                                               == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                   >> 0xcU)) 
                                              & (IData)(vlSelfRef.tag_rd_data_3))))) {
                                    if ((1U & (~ ((
                                                   (0xfffffffffffffULL 
                                                    & (vlSelfRef.tag_rd_data_4 
                                                       >> 2U)) 
                                                   == 
                                                   (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                    >> 0xcU)) 
                                                  & (IData)(vlSelfRef.tag_rd_data_4))))) {
                                        if ((1U & (~ 
                                                   (((0xfffffffffffffULL 
                                                      & (vlSelfRef.tag_rd_data_5 
                                                         >> 2U)) 
                                                     == 
                                                     (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                      >> 0xcU)) 
                                                    & (IData)(vlSelfRef.tag_rd_data_5))))) {
                                            if ((1U 
                                                 & (~ 
                                                    (((0xfffffffffffffULL 
                                                       & (vlSelfRef.tag_rd_data_6 
                                                          >> 2U)) 
                                                      == 
                                                      (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                       >> 0xcU)) 
                                                     & (IData)(vlSelfRef.tag_rd_data_6))))) {
                                                if (
                                                    (1U 
                                                     & (~ 
                                                        (((0xfffffffffffffULL 
                                                           & (vlSelfRef.tag_rd_data_7 
                                                              >> 2U)) 
                                                          == 
                                                          (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                           >> 0xcU)) 
                                                         & (IData)(vlSelfRef.tag_rd_data_7))))) {
                                                    vlSelfRef.data_rd_en = 0U;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    vlSelfRef.data_rd_addr = 0U;
                    if ((((0xfffffffffffffULL & (vlSelfRef.tag_rd_data_0 
                                                 >> 2U)) 
                          == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                              >> 0xcU)) & (IData)(vlSelfRef.tag_rd_data_0))) {
                        vlSelfRef.data_rd_addr = ((0xfc0U 
                                                   & ((IData)(
                                                              (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                               >> 6U)) 
                                                      << 6U)) 
                                                  | (7U 
                                                     & (IData)(
                                                               (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                >> 3U))));
                    } else if ((((0xfffffffffffffULL 
                                  & (vlSelfRef.tag_rd_data_1 
                                     >> 2U)) == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                 >> 0xcU)) 
                                & (IData)(vlSelfRef.tag_rd_data_1))) {
                        vlSelfRef.data_rd_addr = (8U 
                                                  | ((0xfc0U 
                                                      & ((IData)(
                                                                 (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                  >> 6U)) 
                                                         << 6U)) 
                                                     | (7U 
                                                        & (IData)(
                                                                  (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                   >> 3U)))));
                    } else if ((((0xfffffffffffffULL 
                                  & (vlSelfRef.tag_rd_data_2 
                                     >> 2U)) == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                 >> 0xcU)) 
                                & (IData)(vlSelfRef.tag_rd_data_2))) {
                        vlSelfRef.data_rd_addr = (0x10U 
                                                  | ((0xfc0U 
                                                      & ((IData)(
                                                                 (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                  >> 6U)) 
                                                         << 6U)) 
                                                     | (7U 
                                                        & (IData)(
                                                                  (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                   >> 3U)))));
                    } else if ((((0xfffffffffffffULL 
                                  & (vlSelfRef.tag_rd_data_3 
                                     >> 2U)) == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                 >> 0xcU)) 
                                & (IData)(vlSelfRef.tag_rd_data_3))) {
                        vlSelfRef.data_rd_addr = (0x18U 
                                                  | ((0xfc0U 
                                                      & ((IData)(
                                                                 (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                  >> 6U)) 
                                                         << 6U)) 
                                                     | (7U 
                                                        & (IData)(
                                                                  (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                   >> 3U)))));
                    } else if ((((0xfffffffffffffULL 
                                  & (vlSelfRef.tag_rd_data_4 
                                     >> 2U)) == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                 >> 0xcU)) 
                                & (IData)(vlSelfRef.tag_rd_data_4))) {
                        vlSelfRef.data_rd_addr = (0x20U 
                                                  | ((0xfc0U 
                                                      & ((IData)(
                                                                 (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                  >> 6U)) 
                                                         << 6U)) 
                                                     | (7U 
                                                        & (IData)(
                                                                  (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                   >> 3U)))));
                    } else if ((((0xfffffffffffffULL 
                                  & (vlSelfRef.tag_rd_data_5 
                                     >> 2U)) == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                 >> 0xcU)) 
                                & (IData)(vlSelfRef.tag_rd_data_5))) {
                        vlSelfRef.data_rd_addr = (0x28U 
                                                  | ((0xfc0U 
                                                      & ((IData)(
                                                                 (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                  >> 6U)) 
                                                         << 6U)) 
                                                     | (7U 
                                                        & (IData)(
                                                                  (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                   >> 3U)))));
                    } else if ((((0xfffffffffffffULL 
                                  & (vlSelfRef.tag_rd_data_6 
                                     >> 2U)) == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                 >> 0xcU)) 
                                & (IData)(vlSelfRef.tag_rd_data_6))) {
                        vlSelfRef.data_rd_addr = (0x30U 
                                                  | ((0xfc0U 
                                                      & ((IData)(
                                                                 (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                  >> 6U)) 
                                                         << 6U)) 
                                                     | (7U 
                                                        & (IData)(
                                                                  (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                   >> 3U)))));
                    } else if ((((0xfffffffffffffULL 
                                  & (vlSelfRef.tag_rd_data_7 
                                     >> 2U)) == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                 >> 0xcU)) 
                                & (IData)(vlSelfRef.tag_rd_data_7))) {
                        vlSelfRef.data_rd_addr = (0x38U 
                                                  | ((0xfc0U 
                                                      & ((IData)(
                                                                 (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                  >> 6U)) 
                                                         << 6U)) 
                                                     | (7U 
                                                        & (IData)(
                                                                  (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                                   >> 3U)))));
                    }
                }
            }
        }
    }
}

VL_ATTR_COLD void VFsmCacheCtrl___024root___eval_triggers__stl(VFsmCacheCtrl___024root* vlSelf);

VL_ATTR_COLD bool VFsmCacheCtrl___024root___eval_phase__stl(VFsmCacheCtrl___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmCacheCtrl___024root___eval_phase__stl\n"); );
    VFsmCacheCtrl__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Init
    CData/*0:0*/ __VstlExecute;
    // Body
    VFsmCacheCtrl___024root___eval_triggers__stl(vlSelf);
    __VstlExecute = vlSelfRef.__VstlTriggered.any();
    if (__VstlExecute) {
        VFsmCacheCtrl___024root___eval_stl(vlSelf);
    }
    return (__VstlExecute);
}

#ifdef VL_DEBUG
VL_ATTR_COLD void VFsmCacheCtrl___024root___dump_triggers__ico(VFsmCacheCtrl___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmCacheCtrl___024root___dump_triggers__ico\n"); );
    VFsmCacheCtrl__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
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
VL_ATTR_COLD void VFsmCacheCtrl___024root___dump_triggers__act(VFsmCacheCtrl___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmCacheCtrl___024root___dump_triggers__act\n"); );
    VFsmCacheCtrl__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
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
VL_ATTR_COLD void VFsmCacheCtrl___024root___dump_triggers__nba(VFsmCacheCtrl___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmCacheCtrl___024root___dump_triggers__nba\n"); );
    VFsmCacheCtrl__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
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

VL_ATTR_COLD void VFsmCacheCtrl___024root___ctor_var_reset(VFsmCacheCtrl___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmCacheCtrl___024root___ctor_var_reset\n"); );
    VFsmCacheCtrl__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    vlSelf->clk = VL_RAND_RESET_I(1);
    vlSelf->rst = VL_RAND_RESET_I(1);
    vlSelf->req_valid = VL_RAND_RESET_I(1);
    vlSelf->req_ready = VL_RAND_RESET_I(1);
    vlSelf->req_vaddr = VL_RAND_RESET_Q(64);
    vlSelf->req_data = VL_RAND_RESET_Q(64);
    vlSelf->req_be = VL_RAND_RESET_I(8);
    vlSelf->req_is_store = VL_RAND_RESET_I(1);
    vlSelf->resp_valid = VL_RAND_RESET_I(1);
    vlSelf->resp_data = VL_RAND_RESET_Q(64);
    vlSelf->resp_error = VL_RAND_RESET_I(1);
    vlSelf->tag_rd_en_0 = VL_RAND_RESET_I(1);
    vlSelf->tag_rd_addr_0 = VL_RAND_RESET_I(6);
    vlSelf->tag_rd_data_0 = VL_RAND_RESET_Q(54);
    vlSelf->tag_rd_en_1 = VL_RAND_RESET_I(1);
    vlSelf->tag_rd_addr_1 = VL_RAND_RESET_I(6);
    vlSelf->tag_rd_data_1 = VL_RAND_RESET_Q(54);
    vlSelf->tag_rd_en_2 = VL_RAND_RESET_I(1);
    vlSelf->tag_rd_addr_2 = VL_RAND_RESET_I(6);
    vlSelf->tag_rd_data_2 = VL_RAND_RESET_Q(54);
    vlSelf->tag_rd_en_3 = VL_RAND_RESET_I(1);
    vlSelf->tag_rd_addr_3 = VL_RAND_RESET_I(6);
    vlSelf->tag_rd_data_3 = VL_RAND_RESET_Q(54);
    vlSelf->tag_rd_en_4 = VL_RAND_RESET_I(1);
    vlSelf->tag_rd_addr_4 = VL_RAND_RESET_I(6);
    vlSelf->tag_rd_data_4 = VL_RAND_RESET_Q(54);
    vlSelf->tag_rd_en_5 = VL_RAND_RESET_I(1);
    vlSelf->tag_rd_addr_5 = VL_RAND_RESET_I(6);
    vlSelf->tag_rd_data_5 = VL_RAND_RESET_Q(54);
    vlSelf->tag_rd_en_6 = VL_RAND_RESET_I(1);
    vlSelf->tag_rd_addr_6 = VL_RAND_RESET_I(6);
    vlSelf->tag_rd_data_6 = VL_RAND_RESET_Q(54);
    vlSelf->tag_rd_en_7 = VL_RAND_RESET_I(1);
    vlSelf->tag_rd_addr_7 = VL_RAND_RESET_I(6);
    vlSelf->tag_rd_data_7 = VL_RAND_RESET_Q(54);
    vlSelf->tag_wr_en_0 = VL_RAND_RESET_I(1);
    vlSelf->tag_wr_addr_0 = VL_RAND_RESET_I(6);
    vlSelf->tag_wr_data_0 = VL_RAND_RESET_Q(54);
    vlSelf->tag_wr_en_1 = VL_RAND_RESET_I(1);
    vlSelf->tag_wr_addr_1 = VL_RAND_RESET_I(6);
    vlSelf->tag_wr_data_1 = VL_RAND_RESET_Q(54);
    vlSelf->tag_wr_en_2 = VL_RAND_RESET_I(1);
    vlSelf->tag_wr_addr_2 = VL_RAND_RESET_I(6);
    vlSelf->tag_wr_data_2 = VL_RAND_RESET_Q(54);
    vlSelf->tag_wr_en_3 = VL_RAND_RESET_I(1);
    vlSelf->tag_wr_addr_3 = VL_RAND_RESET_I(6);
    vlSelf->tag_wr_data_3 = VL_RAND_RESET_Q(54);
    vlSelf->tag_wr_en_4 = VL_RAND_RESET_I(1);
    vlSelf->tag_wr_addr_4 = VL_RAND_RESET_I(6);
    vlSelf->tag_wr_data_4 = VL_RAND_RESET_Q(54);
    vlSelf->tag_wr_en_5 = VL_RAND_RESET_I(1);
    vlSelf->tag_wr_addr_5 = VL_RAND_RESET_I(6);
    vlSelf->tag_wr_data_5 = VL_RAND_RESET_Q(54);
    vlSelf->tag_wr_en_6 = VL_RAND_RESET_I(1);
    vlSelf->tag_wr_addr_6 = VL_RAND_RESET_I(6);
    vlSelf->tag_wr_data_6 = VL_RAND_RESET_Q(54);
    vlSelf->tag_wr_en_7 = VL_RAND_RESET_I(1);
    vlSelf->tag_wr_addr_7 = VL_RAND_RESET_I(6);
    vlSelf->tag_wr_data_7 = VL_RAND_RESET_Q(54);
    vlSelf->data_rd_en = VL_RAND_RESET_I(1);
    vlSelf->data_rd_addr = VL_RAND_RESET_I(12);
    vlSelf->data_rd_data = VL_RAND_RESET_Q(64);
    vlSelf->data_wr_en = VL_RAND_RESET_I(1);
    vlSelf->data_wr_addr = VL_RAND_RESET_I(12);
    vlSelf->data_wr_data = VL_RAND_RESET_Q(64);
    vlSelf->lru_rd_en = VL_RAND_RESET_I(1);
    vlSelf->lru_rd_addr = VL_RAND_RESET_I(6);
    vlSelf->lru_rd_data = VL_RAND_RESET_I(7);
    vlSelf->lru_wr_en = VL_RAND_RESET_I(1);
    vlSelf->lru_wr_addr = VL_RAND_RESET_I(6);
    vlSelf->lru_wr_data = VL_RAND_RESET_I(7);
    vlSelf->lru_tree_in = VL_RAND_RESET_I(7);
    vlSelf->lru_access_way = VL_RAND_RESET_I(3);
    vlSelf->lru_access_en = VL_RAND_RESET_I(1);
    vlSelf->lru_tree_out = VL_RAND_RESET_I(7);
    vlSelf->lru_victim_way = VL_RAND_RESET_I(3);
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
    vlSelf->FsmCacheCtrl__DOT__state_r = VL_RAND_RESET_I(4);
    vlSelf->FsmCacheCtrl__DOT__state_next = VL_RAND_RESET_I(4);
    vlSelf->FsmCacheCtrl__DOT__req_addr_r = VL_RAND_RESET_Q(64);
    vlSelf->FsmCacheCtrl__DOT__req_data_r = VL_RAND_RESET_Q(64);
    vlSelf->FsmCacheCtrl__DOT__req_is_store_r = VL_RAND_RESET_I(1);
    vlSelf->FsmCacheCtrl__DOT__hit_way_r = VL_RAND_RESET_I(3);
    vlSelf->FsmCacheCtrl__DOT__victim_way_r = VL_RAND_RESET_I(3);
    vlSelf->FsmCacheCtrl__DOT__victim_tag_r = VL_RAND_RESET_Q(52);
    vlSelf->FsmCacheCtrl__DOT__lru_tree_r = VL_RAND_RESET_I(7);
    vlSelf->FsmCacheCtrl__DOT__miss_is_store_r = VL_RAND_RESET_I(1);
    vlSelf->FsmCacheCtrl__DOT__beat_ctr_r = VL_RAND_RESET_I(4);
    vlSelf->FsmCacheCtrl__DOT__wb_buf_0 = VL_RAND_RESET_Q(64);
    vlSelf->FsmCacheCtrl__DOT__wb_buf_1 = VL_RAND_RESET_Q(64);
    vlSelf->FsmCacheCtrl__DOT__wb_buf_2 = VL_RAND_RESET_Q(64);
    vlSelf->FsmCacheCtrl__DOT__wb_buf_3 = VL_RAND_RESET_Q(64);
    vlSelf->FsmCacheCtrl__DOT__wb_buf_4 = VL_RAND_RESET_Q(64);
    vlSelf->FsmCacheCtrl__DOT__wb_buf_5 = VL_RAND_RESET_Q(64);
    vlSelf->FsmCacheCtrl__DOT__wb_buf_6 = VL_RAND_RESET_Q(64);
    vlSelf->FsmCacheCtrl__DOT__wb_buf_7 = VL_RAND_RESET_Q(64);
    vlSelf->FsmCacheCtrl__DOT__lookup_hit_r = VL_RAND_RESET_I(1);
    vlSelf->FsmCacheCtrl__DOT__lookup_victim_dirty_r = VL_RAND_RESET_I(1);
    vlSelf->__Vtrigprevexpr___TOP__clk__0 = VL_RAND_RESET_I(1);
}
