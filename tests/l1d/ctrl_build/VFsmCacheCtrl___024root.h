// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Design internal header
// See VFsmCacheCtrl.h for the primary calling header

#ifndef VERILATED_VFSMCACHECTRL___024ROOT_H_
#define VERILATED_VFSMCACHECTRL___024ROOT_H_  // guard

#include "verilated.h"


class VFsmCacheCtrl__Syms;

class alignas(VL_CACHE_LINE_BYTES) VFsmCacheCtrl___024root final : public VerilatedModule {
  public:

    // DESIGN SPECIFIC STATE
    // Anonymous structures to workaround compiler member-count bugs
    struct {
        VL_IN8(clk,0,0);
        VL_IN8(rst,0,0);
        VL_IN8(req_valid,0,0);
        VL_OUT8(req_ready,0,0);
        VL_IN8(req_be,7,0);
        VL_IN8(req_is_store,0,0);
        VL_OUT8(resp_valid,0,0);
        VL_OUT8(resp_error,0,0);
        VL_OUT8(tag_rd_en_0,0,0);
        VL_OUT8(tag_rd_addr_0,5,0);
        VL_OUT8(tag_rd_en_1,0,0);
        VL_OUT8(tag_rd_addr_1,5,0);
        VL_OUT8(tag_rd_en_2,0,0);
        VL_OUT8(tag_rd_addr_2,5,0);
        VL_OUT8(tag_rd_en_3,0,0);
        VL_OUT8(tag_rd_addr_3,5,0);
        VL_OUT8(tag_rd_en_4,0,0);
        VL_OUT8(tag_rd_addr_4,5,0);
        VL_OUT8(tag_rd_en_5,0,0);
        VL_OUT8(tag_rd_addr_5,5,0);
        VL_OUT8(tag_rd_en_6,0,0);
        VL_OUT8(tag_rd_addr_6,5,0);
        VL_OUT8(tag_rd_en_7,0,0);
        VL_OUT8(tag_rd_addr_7,5,0);
        VL_OUT8(tag_wr_en_0,0,0);
        VL_OUT8(tag_wr_addr_0,5,0);
        VL_OUT8(tag_wr_en_1,0,0);
        VL_OUT8(tag_wr_addr_1,5,0);
        VL_OUT8(tag_wr_en_2,0,0);
        VL_OUT8(tag_wr_addr_2,5,0);
        VL_OUT8(tag_wr_en_3,0,0);
        VL_OUT8(tag_wr_addr_3,5,0);
        VL_OUT8(tag_wr_en_4,0,0);
        VL_OUT8(tag_wr_addr_4,5,0);
        VL_OUT8(tag_wr_en_5,0,0);
        VL_OUT8(tag_wr_addr_5,5,0);
        VL_OUT8(tag_wr_en_6,0,0);
        VL_OUT8(tag_wr_addr_6,5,0);
        VL_OUT8(tag_wr_en_7,0,0);
        VL_OUT8(tag_wr_addr_7,5,0);
        VL_OUT8(data_rd_en,0,0);
        VL_OUT8(data_wr_en,0,0);
        VL_OUT8(lru_rd_en,0,0);
        VL_OUT8(lru_rd_addr,5,0);
        VL_IN8(lru_rd_data,6,0);
        VL_OUT8(lru_wr_en,0,0);
        VL_OUT8(lru_wr_addr,5,0);
        VL_OUT8(lru_wr_data,6,0);
        VL_OUT8(lru_tree_in,6,0);
        VL_OUT8(lru_access_way,2,0);
        VL_OUT8(lru_access_en,0,0);
        VL_IN8(lru_tree_out,6,0);
        VL_IN8(lru_victim_way,2,0);
        VL_OUT8(fill_start,0,0);
        VL_IN8(fill_done,0,0);
        VL_OUT8(wb_start,0,0);
        VL_IN8(wb_done,0,0);
        CData/*3:0*/ FsmCacheCtrl__DOT__state_r;
        CData/*3:0*/ FsmCacheCtrl__DOT__state_next;
        CData/*0:0*/ FsmCacheCtrl__DOT__req_is_store_r;
        CData/*2:0*/ FsmCacheCtrl__DOT__hit_way_r;
        CData/*2:0*/ FsmCacheCtrl__DOT__victim_way_r;
        CData/*6:0*/ FsmCacheCtrl__DOT__lru_tree_r;
        CData/*0:0*/ FsmCacheCtrl__DOT__miss_is_store_r;
    };
    struct {
        CData/*3:0*/ FsmCacheCtrl__DOT__beat_ctr_r;
        CData/*0:0*/ FsmCacheCtrl__DOT__lookup_hit_r;
        CData/*0:0*/ FsmCacheCtrl__DOT__lookup_victim_dirty_r;
        CData/*0:0*/ __VstlFirstIteration;
        CData/*0:0*/ __VicoFirstIteration;
        CData/*0:0*/ __Vtrigprevexpr___TOP__clk__0;
        CData/*0:0*/ __VactContinue;
        VL_OUT16(data_rd_addr,11,0);
        VL_OUT16(data_wr_addr,11,0);
        IData/*31:0*/ __VactIterCount;
        VL_IN64(req_vaddr,63,0);
        VL_IN64(req_data,63,0);
        VL_OUT64(resp_data,63,0);
        VL_IN64(tag_rd_data_0,53,0);
        VL_IN64(tag_rd_data_1,53,0);
        VL_IN64(tag_rd_data_2,53,0);
        VL_IN64(tag_rd_data_3,53,0);
        VL_IN64(tag_rd_data_4,53,0);
        VL_IN64(tag_rd_data_5,53,0);
        VL_IN64(tag_rd_data_6,53,0);
        VL_IN64(tag_rd_data_7,53,0);
        VL_OUT64(tag_wr_data_0,53,0);
        VL_OUT64(tag_wr_data_1,53,0);
        VL_OUT64(tag_wr_data_2,53,0);
        VL_OUT64(tag_wr_data_3,53,0);
        VL_OUT64(tag_wr_data_4,53,0);
        VL_OUT64(tag_wr_data_5,53,0);
        VL_OUT64(tag_wr_data_6,53,0);
        VL_OUT64(tag_wr_data_7,53,0);
        VL_IN64(data_rd_data,63,0);
        VL_OUT64(data_wr_data,63,0);
        VL_OUT64(fill_addr,63,0);
        VL_IN64(fill_word_0,63,0);
        VL_IN64(fill_word_1,63,0);
        VL_IN64(fill_word_2,63,0);
        VL_IN64(fill_word_3,63,0);
        VL_IN64(fill_word_4,63,0);
        VL_IN64(fill_word_5,63,0);
        VL_IN64(fill_word_6,63,0);
        VL_IN64(fill_word_7,63,0);
        VL_OUT64(wb_addr,63,0);
        VL_OUT64(wb_word_0,63,0);
        VL_OUT64(wb_word_1,63,0);
        VL_OUT64(wb_word_2,63,0);
        VL_OUT64(wb_word_3,63,0);
        VL_OUT64(wb_word_4,63,0);
        VL_OUT64(wb_word_5,63,0);
        VL_OUT64(wb_word_6,63,0);
        VL_OUT64(wb_word_7,63,0);
        QData/*63:0*/ FsmCacheCtrl__DOT__req_addr_r;
        QData/*63:0*/ FsmCacheCtrl__DOT__req_data_r;
        QData/*51:0*/ FsmCacheCtrl__DOT__victim_tag_r;
        QData/*63:0*/ FsmCacheCtrl__DOT__wb_buf_0;
        QData/*63:0*/ FsmCacheCtrl__DOT__wb_buf_1;
        QData/*63:0*/ FsmCacheCtrl__DOT__wb_buf_2;
        QData/*63:0*/ FsmCacheCtrl__DOT__wb_buf_3;
        QData/*63:0*/ FsmCacheCtrl__DOT__wb_buf_4;
        QData/*63:0*/ FsmCacheCtrl__DOT__wb_buf_5;
        QData/*63:0*/ FsmCacheCtrl__DOT__wb_buf_6;
        QData/*63:0*/ FsmCacheCtrl__DOT__wb_buf_7;
    };
    VlTriggerVec<1> __VstlTriggered;
    VlTriggerVec<1> __VicoTriggered;
    VlTriggerVec<1> __VactTriggered;
    VlTriggerVec<1> __VnbaTriggered;

    // INTERNAL VARIABLES
    VFsmCacheCtrl__Syms* const vlSymsp;

    // CONSTRUCTORS
    VFsmCacheCtrl___024root(VFsmCacheCtrl__Syms* symsp, const char* v__name);
    ~VFsmCacheCtrl___024root();
    VL_UNCOPYABLE(VFsmCacheCtrl___024root);

    // INTERNAL METHODS
    void __Vconfigure(bool first);
};


#endif  // guard
