// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Design internal header
// See VL1DCache.h for the primary calling header

#ifndef VERILATED_VL1DCACHE___024ROOT_H_
#define VERILATED_VL1DCACHE___024ROOT_H_  // guard

#include "verilated.h"


class VL1DCache__Syms;

class alignas(VL_CACHE_LINE_BYTES) VL1DCache___024root final : public VerilatedModule {
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
        VL_OUT8(ar_valid,0,0);
        VL_IN8(ar_ready,0,0);
        VL_OUT8(ar_id,3,0);
        VL_OUT8(ar_len,7,0);
        VL_OUT8(ar_size,2,0);
        VL_OUT8(ar_burst,1,0);
        VL_IN8(r_valid,0,0);
        VL_OUT8(r_ready,0,0);
        VL_IN8(r_id,3,0);
        VL_IN8(r_resp,1,0);
        VL_IN8(r_last,0,0);
        VL_OUT8(aw_valid,0,0);
        VL_IN8(aw_ready,0,0);
        VL_OUT8(aw_id,3,0);
        VL_OUT8(aw_len,7,0);
        VL_OUT8(aw_size,2,0);
        VL_OUT8(aw_burst,1,0);
        VL_OUT8(w_valid,0,0);
        VL_IN8(w_ready,0,0);
        VL_OUT8(w_strb,7,0);
        VL_OUT8(w_last,0,0);
        VL_IN8(b_valid,0,0);
        VL_OUT8(b_ready,0,0);
        VL_IN8(b_id,3,0);
        VL_IN8(b_resp,1,0);
        CData/*0:0*/ L1DCache__DOT__tag_rd_en_0;
        CData/*5:0*/ L1DCache__DOT__tag_rd_addr_0;
        CData/*0:0*/ L1DCache__DOT__tag_rd_en_1;
        CData/*5:0*/ L1DCache__DOT__tag_rd_addr_1;
        CData/*0:0*/ L1DCache__DOT__tag_rd_en_2;
        CData/*5:0*/ L1DCache__DOT__tag_rd_addr_2;
        CData/*0:0*/ L1DCache__DOT__tag_rd_en_3;
        CData/*5:0*/ L1DCache__DOT__tag_rd_addr_3;
        CData/*0:0*/ L1DCache__DOT__tag_rd_en_4;
        CData/*5:0*/ L1DCache__DOT__tag_rd_addr_4;
        CData/*0:0*/ L1DCache__DOT__tag_rd_en_5;
        CData/*5:0*/ L1DCache__DOT__tag_rd_addr_5;
        CData/*0:0*/ L1DCache__DOT__tag_rd_en_6;
        CData/*5:0*/ L1DCache__DOT__tag_rd_addr_6;
        CData/*0:0*/ L1DCache__DOT__tag_rd_en_7;
        CData/*5:0*/ L1DCache__DOT__tag_rd_addr_7;
        CData/*0:0*/ L1DCache__DOT__tag_wr_en_0;
        CData/*5:0*/ L1DCache__DOT__tag_wr_addr_0;
        CData/*0:0*/ L1DCache__DOT__tag_wr_en_1;
        CData/*5:0*/ L1DCache__DOT__tag_wr_addr_1;
        CData/*0:0*/ L1DCache__DOT__tag_wr_en_2;
        CData/*5:0*/ L1DCache__DOT__tag_wr_addr_2;
        CData/*0:0*/ L1DCache__DOT__tag_wr_en_3;
        CData/*5:0*/ L1DCache__DOT__tag_wr_addr_3;
        CData/*0:0*/ L1DCache__DOT__tag_wr_en_4;
        CData/*5:0*/ L1DCache__DOT__tag_wr_addr_4;
        CData/*0:0*/ L1DCache__DOT__tag_wr_en_5;
        CData/*5:0*/ L1DCache__DOT__tag_wr_addr_5;
        CData/*0:0*/ L1DCache__DOT__tag_wr_en_6;
        CData/*5:0*/ L1DCache__DOT__tag_wr_addr_6;
        CData/*0:0*/ L1DCache__DOT__tag_wr_en_7;
    };
    struct {
        CData/*5:0*/ L1DCache__DOT__tag_wr_addr_7;
        CData/*0:0*/ L1DCache__DOT__data_rd_en_w;
        CData/*0:0*/ L1DCache__DOT__data_wr_en_w;
        CData/*0:0*/ L1DCache__DOT__lru_rd_en_w;
        CData/*5:0*/ L1DCache__DOT__lru_rd_addr_w;
        CData/*0:0*/ L1DCache__DOT__lru_wr_en_w;
        CData/*5:0*/ L1DCache__DOT__lru_wr_addr_w;
        CData/*6:0*/ L1DCache__DOT__lru_wr_data_w;
        CData/*0:0*/ L1DCache__DOT__fill_start_w;
        CData/*0:0*/ L1DCache__DOT__fill_done_w;
        CData/*0:0*/ L1DCache__DOT__wb_start_w;
        CData/*6:0*/ L1DCache__DOT__lru_ram__DOT__rd_port_rdata_r;
        CData/*2:0*/ L1DCache__DOT__lru_upd__DOT__idx;
        CData/*3:0*/ L1DCache__DOT__ctrl__DOT__state_r;
        CData/*3:0*/ L1DCache__DOT__ctrl__DOT__state_next;
        CData/*0:0*/ L1DCache__DOT__ctrl__DOT__req_is_store_r;
        CData/*2:0*/ L1DCache__DOT__ctrl__DOT__hit_way_r;
        CData/*2:0*/ L1DCache__DOT__ctrl__DOT__victim_way_r;
        CData/*6:0*/ L1DCache__DOT__ctrl__DOT__lru_tree_r;
        CData/*0:0*/ L1DCache__DOT__ctrl__DOT__miss_is_store_r;
        CData/*3:0*/ L1DCache__DOT__ctrl__DOT__beat_ctr_r;
        CData/*0:0*/ L1DCache__DOT__ctrl__DOT__lookup_hit_r;
        CData/*0:0*/ L1DCache__DOT__ctrl__DOT__lookup_victim_dirty_r;
        CData/*1:0*/ L1DCache__DOT__fill_fsm__DOT__state_r;
        CData/*1:0*/ L1DCache__DOT__fill_fsm__DOT__state_next;
        CData/*3:0*/ L1DCache__DOT__fill_fsm__DOT__beat_ctr_r;
        CData/*1:0*/ L1DCache__DOT__wb_fsm__DOT__state_r;
        CData/*1:0*/ L1DCache__DOT__wb_fsm__DOT__state_next;
        CData/*3:0*/ L1DCache__DOT__wb_fsm__DOT__beat_ctr_r;
        CData/*0:0*/ __VstlFirstIteration;
        CData/*0:0*/ __VicoFirstIteration;
        CData/*0:0*/ __Vtrigprevexpr___TOP__clk__0;
        CData/*0:0*/ __VactContinue;
        SData/*11:0*/ L1DCache__DOT__data_rd_addr_w;
        SData/*11:0*/ L1DCache__DOT__data_wr_addr_w;
        IData/*31:0*/ __VactIterCount;
        VL_IN64(req_vaddr,63,0);
        VL_IN64(req_data,63,0);
        VL_OUT64(resp_data,63,0);
        VL_OUT64(ar_addr,63,0);
        VL_IN64(r_data,63,0);
        VL_OUT64(aw_addr,63,0);
        VL_OUT64(w_data,63,0);
        QData/*53:0*/ L1DCache__DOT__tag_wr_data_0;
        QData/*53:0*/ L1DCache__DOT__tag_wr_data_1;
        QData/*53:0*/ L1DCache__DOT__tag_wr_data_2;
        QData/*53:0*/ L1DCache__DOT__tag_wr_data_3;
        QData/*53:0*/ L1DCache__DOT__tag_wr_data_4;
        QData/*53:0*/ L1DCache__DOT__tag_wr_data_5;
        QData/*53:0*/ L1DCache__DOT__tag_wr_data_6;
        QData/*53:0*/ L1DCache__DOT__tag_wr_data_7;
        QData/*63:0*/ L1DCache__DOT__data_wr_data_w;
        QData/*63:0*/ L1DCache__DOT__fill_addr_w;
        QData/*63:0*/ L1DCache__DOT__fill_word_0_w;
        QData/*63:0*/ L1DCache__DOT__fill_word_1_w;
        QData/*63:0*/ L1DCache__DOT__fill_word_2_w;
        QData/*63:0*/ L1DCache__DOT__fill_word_3_w;
        QData/*63:0*/ L1DCache__DOT__fill_word_4_w;
        QData/*63:0*/ L1DCache__DOT__fill_word_5_w;
        QData/*63:0*/ L1DCache__DOT__fill_word_6_w;
        QData/*63:0*/ L1DCache__DOT__fill_word_7_w;
        QData/*63:0*/ L1DCache__DOT__wb_addr_w;
        QData/*53:0*/ L1DCache__DOT__tag_0__DOT__rd_port_rdata_r;
        QData/*53:0*/ L1DCache__DOT__tag_1__DOT__rd_port_rdata_r;
    };
    struct {
        QData/*53:0*/ L1DCache__DOT__tag_2__DOT__rd_port_rdata_r;
        QData/*53:0*/ L1DCache__DOT__tag_3__DOT__rd_port_rdata_r;
        QData/*53:0*/ L1DCache__DOT__tag_4__DOT__rd_port_rdata_r;
        QData/*53:0*/ L1DCache__DOT__tag_5__DOT__rd_port_rdata_r;
        QData/*53:0*/ L1DCache__DOT__tag_6__DOT__rd_port_rdata_r;
        QData/*53:0*/ L1DCache__DOT__tag_7__DOT__rd_port_rdata_r;
        QData/*63:0*/ L1DCache__DOT__data_ram__DOT__rd_port_rdata_r;
        QData/*63:0*/ L1DCache__DOT__ctrl__DOT__req_addr_r;
        QData/*63:0*/ L1DCache__DOT__ctrl__DOT__req_data_r;
        QData/*51:0*/ L1DCache__DOT__ctrl__DOT__victim_tag_r;
        QData/*63:0*/ L1DCache__DOT__ctrl__DOT__wb_buf_0;
        QData/*63:0*/ L1DCache__DOT__ctrl__DOT__wb_buf_1;
        QData/*63:0*/ L1DCache__DOT__ctrl__DOT__wb_buf_2;
        QData/*63:0*/ L1DCache__DOT__ctrl__DOT__wb_buf_3;
        QData/*63:0*/ L1DCache__DOT__ctrl__DOT__wb_buf_4;
        QData/*63:0*/ L1DCache__DOT__ctrl__DOT__wb_buf_5;
        QData/*63:0*/ L1DCache__DOT__ctrl__DOT__wb_buf_6;
        QData/*63:0*/ L1DCache__DOT__ctrl__DOT__wb_buf_7;
        QData/*63:0*/ L1DCache__DOT__fill_fsm__DOT__fill_addr_r;
        QData/*63:0*/ L1DCache__DOT__wb_fsm__DOT__wb_addr_r;
        VlUnpacked<QData/*53:0*/, 64> L1DCache__DOT__tag_0__DOT__mem;
        VlUnpacked<QData/*53:0*/, 64> L1DCache__DOT__tag_1__DOT__mem;
        VlUnpacked<QData/*53:0*/, 64> L1DCache__DOT__tag_2__DOT__mem;
        VlUnpacked<QData/*53:0*/, 64> L1DCache__DOT__tag_3__DOT__mem;
        VlUnpacked<QData/*53:0*/, 64> L1DCache__DOT__tag_4__DOT__mem;
        VlUnpacked<QData/*53:0*/, 64> L1DCache__DOT__tag_5__DOT__mem;
        VlUnpacked<QData/*53:0*/, 64> L1DCache__DOT__tag_6__DOT__mem;
        VlUnpacked<QData/*53:0*/, 64> L1DCache__DOT__tag_7__DOT__mem;
        VlUnpacked<QData/*63:0*/, 4096> L1DCache__DOT__data_ram__DOT__mem;
        VlUnpacked<CData/*6:0*/, 64> L1DCache__DOT__lru_ram__DOT__mem;
    };
    VlTriggerVec<1> __VstlTriggered;
    VlTriggerVec<1> __VicoTriggered;
    VlTriggerVec<1> __VactTriggered;
    VlTriggerVec<1> __VnbaTriggered;

    // INTERNAL VARIABLES
    VL1DCache__Syms* const vlSymsp;

    // CONSTRUCTORS
    VL1DCache___024root(VL1DCache__Syms* symsp, const char* v__name);
    ~VL1DCache___024root();
    VL_UNCOPYABLE(VL1DCache___024root);

    // INTERNAL METHODS
    void __Vconfigure(bool first);
};


#endif  // guard
