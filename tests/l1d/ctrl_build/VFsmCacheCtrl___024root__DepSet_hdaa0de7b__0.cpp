// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Design implementation internals
// See VFsmCacheCtrl.h for the primary calling header

#include "VFsmCacheCtrl__pch.h"
#include "VFsmCacheCtrl___024root.h"

void VFsmCacheCtrl___024root___ico_sequent__TOP__0(VFsmCacheCtrl___024root* vlSelf);

void VFsmCacheCtrl___024root___eval_ico(VFsmCacheCtrl___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmCacheCtrl___024root___eval_ico\n"); );
    VFsmCacheCtrl__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    if ((1ULL & vlSelfRef.__VicoTriggered.word(0U))) {
        VFsmCacheCtrl___024root___ico_sequent__TOP__0(vlSelf);
    }
}

VL_INLINE_OPT void VFsmCacheCtrl___024root___ico_sequent__TOP__0(VFsmCacheCtrl___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmCacheCtrl___024root___ico_sequent__TOP__0\n"); );
    VFsmCacheCtrl__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    vlSelfRef.tag_rd_en_0 = 0U;
    vlSelfRef.tag_rd_en_1 = 0U;
    vlSelfRef.tag_rd_en_2 = 0U;
    vlSelfRef.tag_rd_en_3 = 0U;
    vlSelfRef.tag_rd_en_4 = 0U;
    vlSelfRef.tag_rd_en_5 = 0U;
    vlSelfRef.tag_rd_en_6 = 0U;
    vlSelfRef.tag_rd_en_7 = 0U;
    vlSelfRef.lru_rd_en = 0U;
    vlSelfRef.tag_rd_addr_0 = 0U;
    vlSelfRef.tag_rd_addr_1 = 0U;
    vlSelfRef.tag_rd_addr_2 = 0U;
    vlSelfRef.tag_rd_addr_3 = 0U;
    vlSelfRef.tag_rd_addr_4 = 0U;
    vlSelfRef.tag_rd_addr_5 = 0U;
    vlSelfRef.tag_rd_addr_6 = 0U;
    vlSelfRef.lru_rd_addr = 0U;
    vlSelfRef.lru_wr_data = 0U;
    vlSelfRef.tag_rd_addr_7 = 0U;
    vlSelfRef.lru_tree_in = 0U;
    vlSelfRef.FsmCacheCtrl__DOT__state_next = vlSelfRef.FsmCacheCtrl__DOT__state_r;
    if ((8U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
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
                    vlSelfRef.lru_wr_data = vlSelfRef.lru_tree_out;
                    vlSelfRef.lru_tree_in = vlSelfRef.FsmCacheCtrl__DOT__lru_tree_r;
                } else if (vlSelfRef.FsmCacheCtrl__DOT__lookup_hit_r) {
                    vlSelfRef.lru_wr_data = vlSelfRef.lru_tree_out;
                    vlSelfRef.lru_tree_in = vlSelfRef.FsmCacheCtrl__DOT__lru_tree_r;
                }
            } else if ((1U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
                vlSelfRef.lru_tree_in = vlSelfRef.lru_rd_data;
            }
        }
        if ((4U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
            if ((2U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
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
                if ((1U & (~ (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r)))) {
                    vlSelfRef.data_wr_data = vlSelfRef.FsmCacheCtrl__DOT__req_data_r;
                    vlSelfRef.resp_data = vlSelfRef.FsmCacheCtrl__DOT__req_data_r;
                }
            } else if ((1U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
                vlSelfRef.data_wr_data = vlSelfRef.fill_word_0;
                if ((0U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                    vlSelfRef.data_wr_data = vlSelfRef.fill_word_0;
                } else if ((1U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                    vlSelfRef.data_wr_data = vlSelfRef.fill_word_1;
                } else if ((2U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                    vlSelfRef.data_wr_data = vlSelfRef.fill_word_2;
                } else if ((3U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                    vlSelfRef.data_wr_data = vlSelfRef.fill_word_3;
                } else if ((4U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                    vlSelfRef.data_wr_data = vlSelfRef.fill_word_4;
                } else if ((5U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                    vlSelfRef.data_wr_data = vlSelfRef.fill_word_5;
                } else if ((6U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                    vlSelfRef.data_wr_data = vlSelfRef.fill_word_6;
                } else if ((7U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                    vlSelfRef.data_wr_data = vlSelfRef.fill_word_7;
                }
                if (((7U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r)) 
                     & (~ (IData)(vlSelfRef.FsmCacheCtrl__DOT__miss_is_store_r)))) {
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
            }
        } else {
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
            if ((2U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
                if ((1U & (~ (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r)))) {
                    if (vlSelfRef.FsmCacheCtrl__DOT__lookup_hit_r) {
                        if (vlSelfRef.FsmCacheCtrl__DOT__req_is_store_r) {
                            vlSelfRef.data_wr_data 
                                = vlSelfRef.FsmCacheCtrl__DOT__req_data_r;
                        }
                    }
                    vlSelfRef.resp_data = vlSelfRef.data_rd_data;
                }
            }
        }
    }
}

void VFsmCacheCtrl___024root___eval_triggers__ico(VFsmCacheCtrl___024root* vlSelf);

bool VFsmCacheCtrl___024root___eval_phase__ico(VFsmCacheCtrl___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmCacheCtrl___024root___eval_phase__ico\n"); );
    VFsmCacheCtrl__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Init
    CData/*0:0*/ __VicoExecute;
    // Body
    VFsmCacheCtrl___024root___eval_triggers__ico(vlSelf);
    __VicoExecute = vlSelfRef.__VicoTriggered.any();
    if (__VicoExecute) {
        VFsmCacheCtrl___024root___eval_ico(vlSelf);
    }
    return (__VicoExecute);
}

void VFsmCacheCtrl___024root___eval_act(VFsmCacheCtrl___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmCacheCtrl___024root___eval_act\n"); );
    VFsmCacheCtrl__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
}

void VFsmCacheCtrl___024root___nba_sequent__TOP__0(VFsmCacheCtrl___024root* vlSelf);

void VFsmCacheCtrl___024root___eval_nba(VFsmCacheCtrl___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmCacheCtrl___024root___eval_nba\n"); );
    VFsmCacheCtrl__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    if ((1ULL & vlSelfRef.__VnbaTriggered.word(0U))) {
        VFsmCacheCtrl___024root___nba_sequent__TOP__0(vlSelf);
    }
}

VL_INLINE_OPT void VFsmCacheCtrl___024root___nba_sequent__TOP__0(VFsmCacheCtrl___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmCacheCtrl___024root___nba_sequent__TOP__0\n"); );
    VFsmCacheCtrl__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Init
    CData/*3:0*/ __Vdly__FsmCacheCtrl__DOT__beat_ctr_r;
    __Vdly__FsmCacheCtrl__DOT__beat_ctr_r = 0;
    // Body
    __Vdly__FsmCacheCtrl__DOT__beat_ctr_r = vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r;
    if (vlSelfRef.rst) {
        __Vdly__FsmCacheCtrl__DOT__beat_ctr_r = 0U;
        vlSelfRef.FsmCacheCtrl__DOT__state_r = 0U;
    } else {
        if ((8U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
            if ((1U & (~ ((IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r) 
                          >> 2U)))) {
                if ((1U & (~ ((IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r) 
                              >> 1U)))) {
                    if ((1U & (~ (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r)))) {
                        __Vdly__FsmCacheCtrl__DOT__beat_ctr_r = 0U;
                    }
                }
            }
        } else if ((4U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
            if ((2U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
                if ((1U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
                    __Vdly__FsmCacheCtrl__DOT__beat_ctr_r 
                        = (0xfU & ((IData)(1U) + (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r)));
                    if ((1U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                        vlSelfRef.FsmCacheCtrl__DOT__wb_buf_0 
                            = vlSelfRef.data_rd_data;
                    } else if ((2U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                        vlSelfRef.FsmCacheCtrl__DOT__wb_buf_1 
                            = vlSelfRef.data_rd_data;
                    } else if ((3U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                        vlSelfRef.FsmCacheCtrl__DOT__wb_buf_2 
                            = vlSelfRef.data_rd_data;
                    } else if ((4U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                        vlSelfRef.FsmCacheCtrl__DOT__wb_buf_3 
                            = vlSelfRef.data_rd_data;
                    } else if ((5U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                        vlSelfRef.FsmCacheCtrl__DOT__wb_buf_4 
                            = vlSelfRef.data_rd_data;
                    } else if ((6U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                        vlSelfRef.FsmCacheCtrl__DOT__wb_buf_5 
                            = vlSelfRef.data_rd_data;
                    } else if ((7U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                        vlSelfRef.FsmCacheCtrl__DOT__wb_buf_6 
                            = vlSelfRef.data_rd_data;
                    } else if ((8U == (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r))) {
                        vlSelfRef.FsmCacheCtrl__DOT__wb_buf_7 
                            = vlSelfRef.data_rd_data;
                    }
                }
            } else {
                __Vdly__FsmCacheCtrl__DOT__beat_ctr_r 
                    = ((1U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))
                        ? (0xfU & ((IData)(1U) + (IData)(vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r)))
                        : 0U);
            }
        } else if ((1U & (~ ((IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r) 
                             >> 1U)))) {
            if ((1U & (IData)(vlSelfRef.FsmCacheCtrl__DOT__state_r))) {
                vlSelfRef.FsmCacheCtrl__DOT__hit_way_r = 0U;
                vlSelfRef.FsmCacheCtrl__DOT__lookup_hit_r 
                    = ((((((((((0xfffffffffffffULL 
                                & (vlSelfRef.tag_rd_data_0 
                                   >> 2U)) == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                               >> 0xcU)) 
                              & (IData)(vlSelfRef.tag_rd_data_0)) 
                             | (((0xfffffffffffffULL 
                                  & (vlSelfRef.tag_rd_data_1 
                                     >> 2U)) == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                 >> 0xcU)) 
                                & (IData)(vlSelfRef.tag_rd_data_1))) 
                            | (((0xfffffffffffffULL 
                                 & (vlSelfRef.tag_rd_data_2 
                                    >> 2U)) == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                                >> 0xcU)) 
                               & (IData)(vlSelfRef.tag_rd_data_2))) 
                           | (((0xfffffffffffffULL 
                                & (vlSelfRef.tag_rd_data_3 
                                   >> 2U)) == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                               >> 0xcU)) 
                              & (IData)(vlSelfRef.tag_rd_data_3))) 
                          | (((0xfffffffffffffULL & 
                               (vlSelfRef.tag_rd_data_4 
                                >> 2U)) == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                            >> 0xcU)) 
                             & (IData)(vlSelfRef.tag_rd_data_4))) 
                         | (((0xfffffffffffffULL & 
                              (vlSelfRef.tag_rd_data_5 
                               >> 2U)) == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                           >> 0xcU)) 
                            & (IData)(vlSelfRef.tag_rd_data_5))) 
                        | (((0xfffffffffffffULL & (vlSelfRef.tag_rd_data_6 
                                                   >> 2U)) 
                            == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                >> 0xcU)) & (IData)(vlSelfRef.tag_rd_data_6))) 
                       | (((0xfffffffffffffULL & (vlSelfRef.tag_rd_data_7 
                                                  >> 2U)) 
                           == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                               >> 0xcU)) & (IData)(vlSelfRef.tag_rd_data_7)));
                vlSelfRef.FsmCacheCtrl__DOT__victim_way_r 
                    = vlSelfRef.lru_victim_way;
                vlSelfRef.FsmCacheCtrl__DOT__lru_tree_r 
                    = vlSelfRef.lru_rd_data;
                vlSelfRef.FsmCacheCtrl__DOT__miss_is_store_r 
                    = vlSelfRef.FsmCacheCtrl__DOT__req_is_store_r;
                vlSelfRef.FsmCacheCtrl__DOT__victim_tag_r = 0ULL;
                vlSelfRef.FsmCacheCtrl__DOT__lookup_victim_dirty_r = 0U;
                if ((((0xfffffffffffffULL & (vlSelfRef.tag_rd_data_1 
                                             >> 2U)) 
                      == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                          >> 0xcU)) & (IData)(vlSelfRef.tag_rd_data_1))) {
                    vlSelfRef.FsmCacheCtrl__DOT__hit_way_r = 1U;
                } else if ((((0xfffffffffffffULL & 
                              (vlSelfRef.tag_rd_data_2 
                               >> 2U)) == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                           >> 0xcU)) 
                            & (IData)(vlSelfRef.tag_rd_data_2))) {
                    vlSelfRef.FsmCacheCtrl__DOT__hit_way_r = 2U;
                } else if ((((0xfffffffffffffULL & 
                              (vlSelfRef.tag_rd_data_3 
                               >> 2U)) == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                           >> 0xcU)) 
                            & (IData)(vlSelfRef.tag_rd_data_3))) {
                    vlSelfRef.FsmCacheCtrl__DOT__hit_way_r = 3U;
                } else if ((((0xfffffffffffffULL & 
                              (vlSelfRef.tag_rd_data_4 
                               >> 2U)) == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                           >> 0xcU)) 
                            & (IData)(vlSelfRef.tag_rd_data_4))) {
                    vlSelfRef.FsmCacheCtrl__DOT__hit_way_r = 4U;
                } else if ((((0xfffffffffffffULL & 
                              (vlSelfRef.tag_rd_data_5 
                               >> 2U)) == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                           >> 0xcU)) 
                            & (IData)(vlSelfRef.tag_rd_data_5))) {
                    vlSelfRef.FsmCacheCtrl__DOT__hit_way_r = 5U;
                } else if ((((0xfffffffffffffULL & 
                              (vlSelfRef.tag_rd_data_6 
                               >> 2U)) == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                           >> 0xcU)) 
                            & (IData)(vlSelfRef.tag_rd_data_6))) {
                    vlSelfRef.FsmCacheCtrl__DOT__hit_way_r = 6U;
                } else if ((((0xfffffffffffffULL & 
                              (vlSelfRef.tag_rd_data_7 
                               >> 2U)) == (vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                                           >> 0xcU)) 
                            & (IData)(vlSelfRef.tag_rd_data_7))) {
                    vlSelfRef.FsmCacheCtrl__DOT__hit_way_r = 7U;
                }
                if ((0U == (IData)(vlSelfRef.lru_victim_way))) {
                    vlSelfRef.FsmCacheCtrl__DOT__victim_tag_r 
                        = (0xfffffffffffffULL & (vlSelfRef.tag_rd_data_0 
                                                 >> 2U));
                    vlSelfRef.FsmCacheCtrl__DOT__lookup_victim_dirty_r 
                        = (1U & (IData)((vlSelfRef.tag_rd_data_0 
                                         >> 1U)));
                } else if ((1U == (IData)(vlSelfRef.lru_victim_way))) {
                    vlSelfRef.FsmCacheCtrl__DOT__victim_tag_r 
                        = (0xfffffffffffffULL & (vlSelfRef.tag_rd_data_1 
                                                 >> 2U));
                    vlSelfRef.FsmCacheCtrl__DOT__lookup_victim_dirty_r 
                        = (1U & (IData)((vlSelfRef.tag_rd_data_1 
                                         >> 1U)));
                } else if ((2U == (IData)(vlSelfRef.lru_victim_way))) {
                    vlSelfRef.FsmCacheCtrl__DOT__victim_tag_r 
                        = (0xfffffffffffffULL & (vlSelfRef.tag_rd_data_2 
                                                 >> 2U));
                    vlSelfRef.FsmCacheCtrl__DOT__lookup_victim_dirty_r 
                        = (1U & (IData)((vlSelfRef.tag_rd_data_2 
                                         >> 1U)));
                } else if ((3U == (IData)(vlSelfRef.lru_victim_way))) {
                    vlSelfRef.FsmCacheCtrl__DOT__victim_tag_r 
                        = (0xfffffffffffffULL & (vlSelfRef.tag_rd_data_3 
                                                 >> 2U));
                    vlSelfRef.FsmCacheCtrl__DOT__lookup_victim_dirty_r 
                        = (1U & (IData)((vlSelfRef.tag_rd_data_3 
                                         >> 1U)));
                } else if ((4U == (IData)(vlSelfRef.lru_victim_way))) {
                    vlSelfRef.FsmCacheCtrl__DOT__victim_tag_r 
                        = (0xfffffffffffffULL & (vlSelfRef.tag_rd_data_4 
                                                 >> 2U));
                    vlSelfRef.FsmCacheCtrl__DOT__lookup_victim_dirty_r 
                        = (1U & (IData)((vlSelfRef.tag_rd_data_4 
                                         >> 1U)));
                } else if ((5U == (IData)(vlSelfRef.lru_victim_way))) {
                    vlSelfRef.FsmCacheCtrl__DOT__victim_tag_r 
                        = (0xfffffffffffffULL & (vlSelfRef.tag_rd_data_5 
                                                 >> 2U));
                    vlSelfRef.FsmCacheCtrl__DOT__lookup_victim_dirty_r 
                        = (1U & (IData)((vlSelfRef.tag_rd_data_5 
                                         >> 1U)));
                } else if ((6U == (IData)(vlSelfRef.lru_victim_way))) {
                    vlSelfRef.FsmCacheCtrl__DOT__victim_tag_r 
                        = (0xfffffffffffffULL & (vlSelfRef.tag_rd_data_6 
                                                 >> 2U));
                    vlSelfRef.FsmCacheCtrl__DOT__lookup_victim_dirty_r 
                        = (1U & (IData)((vlSelfRef.tag_rd_data_6 
                                         >> 1U)));
                } else if ((7U == (IData)(vlSelfRef.lru_victim_way))) {
                    vlSelfRef.FsmCacheCtrl__DOT__victim_tag_r 
                        = (0xfffffffffffffULL & (vlSelfRef.tag_rd_data_7 
                                                 >> 2U));
                    vlSelfRef.FsmCacheCtrl__DOT__lookup_victim_dirty_r 
                        = (1U & (IData)((vlSelfRef.tag_rd_data_7 
                                         >> 1U)));
                }
            } else if (vlSelfRef.req_valid) {
                vlSelfRef.FsmCacheCtrl__DOT__req_addr_r 
                    = vlSelfRef.req_vaddr;
                vlSelfRef.FsmCacheCtrl__DOT__req_data_r 
                    = vlSelfRef.req_data;
                vlSelfRef.FsmCacheCtrl__DOT__req_is_store_r 
                    = vlSelfRef.req_is_store;
                __Vdly__FsmCacheCtrl__DOT__beat_ctr_r = 0U;
            }
        }
        vlSelfRef.FsmCacheCtrl__DOT__state_r = vlSelfRef.FsmCacheCtrl__DOT__state_next;
    }
    vlSelfRef.FsmCacheCtrl__DOT__beat_ctr_r = __Vdly__FsmCacheCtrl__DOT__beat_ctr_r;
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

void VFsmCacheCtrl___024root___eval_triggers__act(VFsmCacheCtrl___024root* vlSelf);

bool VFsmCacheCtrl___024root___eval_phase__act(VFsmCacheCtrl___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmCacheCtrl___024root___eval_phase__act\n"); );
    VFsmCacheCtrl__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Init
    VlTriggerVec<1> __VpreTriggered;
    CData/*0:0*/ __VactExecute;
    // Body
    VFsmCacheCtrl___024root___eval_triggers__act(vlSelf);
    __VactExecute = vlSelfRef.__VactTriggered.any();
    if (__VactExecute) {
        __VpreTriggered.andNot(vlSelfRef.__VactTriggered, vlSelfRef.__VnbaTriggered);
        vlSelfRef.__VnbaTriggered.thisOr(vlSelfRef.__VactTriggered);
        VFsmCacheCtrl___024root___eval_act(vlSelf);
    }
    return (__VactExecute);
}

bool VFsmCacheCtrl___024root___eval_phase__nba(VFsmCacheCtrl___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmCacheCtrl___024root___eval_phase__nba\n"); );
    VFsmCacheCtrl__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Init
    CData/*0:0*/ __VnbaExecute;
    // Body
    __VnbaExecute = vlSelfRef.__VnbaTriggered.any();
    if (__VnbaExecute) {
        VFsmCacheCtrl___024root___eval_nba(vlSelf);
        vlSelfRef.__VnbaTriggered.clear();
    }
    return (__VnbaExecute);
}

#ifdef VL_DEBUG
VL_ATTR_COLD void VFsmCacheCtrl___024root___dump_triggers__ico(VFsmCacheCtrl___024root* vlSelf);
#endif  // VL_DEBUG
#ifdef VL_DEBUG
VL_ATTR_COLD void VFsmCacheCtrl___024root___dump_triggers__nba(VFsmCacheCtrl___024root* vlSelf);
#endif  // VL_DEBUG
#ifdef VL_DEBUG
VL_ATTR_COLD void VFsmCacheCtrl___024root___dump_triggers__act(VFsmCacheCtrl___024root* vlSelf);
#endif  // VL_DEBUG

void VFsmCacheCtrl___024root___eval(VFsmCacheCtrl___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmCacheCtrl___024root___eval\n"); );
    VFsmCacheCtrl__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
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
            VFsmCacheCtrl___024root___dump_triggers__ico(vlSelf);
#endif
            VL_FATAL_MT("tests/l1d/FsmCacheCtrl.sv", 6, "", "Input combinational region did not converge.");
        }
        __VicoIterCount = ((IData)(1U) + __VicoIterCount);
        __VicoContinue = 0U;
        if (VFsmCacheCtrl___024root___eval_phase__ico(vlSelf)) {
            __VicoContinue = 1U;
        }
        vlSelfRef.__VicoFirstIteration = 0U;
    }
    __VnbaIterCount = 0U;
    __VnbaContinue = 1U;
    while (__VnbaContinue) {
        if (VL_UNLIKELY(((0x64U < __VnbaIterCount)))) {
#ifdef VL_DEBUG
            VFsmCacheCtrl___024root___dump_triggers__nba(vlSelf);
#endif
            VL_FATAL_MT("tests/l1d/FsmCacheCtrl.sv", 6, "", "NBA region did not converge.");
        }
        __VnbaIterCount = ((IData)(1U) + __VnbaIterCount);
        __VnbaContinue = 0U;
        vlSelfRef.__VactIterCount = 0U;
        vlSelfRef.__VactContinue = 1U;
        while (vlSelfRef.__VactContinue) {
            if (VL_UNLIKELY(((0x64U < vlSelfRef.__VactIterCount)))) {
#ifdef VL_DEBUG
                VFsmCacheCtrl___024root___dump_triggers__act(vlSelf);
#endif
                VL_FATAL_MT("tests/l1d/FsmCacheCtrl.sv", 6, "", "Active region did not converge.");
            }
            vlSelfRef.__VactIterCount = ((IData)(1U) 
                                         + vlSelfRef.__VactIterCount);
            vlSelfRef.__VactContinue = 0U;
            if (VFsmCacheCtrl___024root___eval_phase__act(vlSelf)) {
                vlSelfRef.__VactContinue = 1U;
            }
        }
        if (VFsmCacheCtrl___024root___eval_phase__nba(vlSelf)) {
            __VnbaContinue = 1U;
        }
    }
}

#ifdef VL_DEBUG
void VFsmCacheCtrl___024root___eval_debug_assertions(VFsmCacheCtrl___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VFsmCacheCtrl___024root___eval_debug_assertions\n"); );
    VFsmCacheCtrl__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
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
    if (VL_UNLIKELY(((vlSelfRef.tag_rd_data_0 & 0ULL)))) {
        Verilated::overWidthError("tag_rd_data_0");}
    if (VL_UNLIKELY(((vlSelfRef.tag_rd_data_1 & 0ULL)))) {
        Verilated::overWidthError("tag_rd_data_1");}
    if (VL_UNLIKELY(((vlSelfRef.tag_rd_data_2 & 0ULL)))) {
        Verilated::overWidthError("tag_rd_data_2");}
    if (VL_UNLIKELY(((vlSelfRef.tag_rd_data_3 & 0ULL)))) {
        Verilated::overWidthError("tag_rd_data_3");}
    if (VL_UNLIKELY(((vlSelfRef.tag_rd_data_4 & 0ULL)))) {
        Verilated::overWidthError("tag_rd_data_4");}
    if (VL_UNLIKELY(((vlSelfRef.tag_rd_data_5 & 0ULL)))) {
        Verilated::overWidthError("tag_rd_data_5");}
    if (VL_UNLIKELY(((vlSelfRef.tag_rd_data_6 & 0ULL)))) {
        Verilated::overWidthError("tag_rd_data_6");}
    if (VL_UNLIKELY(((vlSelfRef.tag_rd_data_7 & 0ULL)))) {
        Verilated::overWidthError("tag_rd_data_7");}
    if (VL_UNLIKELY(((vlSelfRef.lru_rd_data & 0x80U)))) {
        Verilated::overWidthError("lru_rd_data");}
    if (VL_UNLIKELY(((vlSelfRef.lru_tree_out & 0x80U)))) {
        Verilated::overWidthError("lru_tree_out");}
    if (VL_UNLIKELY(((vlSelfRef.lru_victim_way & 0xf8U)))) {
        Verilated::overWidthError("lru_victim_way");}
    if (VL_UNLIKELY(((vlSelfRef.fill_done & 0xfeU)))) {
        Verilated::overWidthError("fill_done");}
    if (VL_UNLIKELY(((vlSelfRef.wb_done & 0xfeU)))) {
        Verilated::overWidthError("wb_done");}
}
#endif  // VL_DEBUG
