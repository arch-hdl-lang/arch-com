// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Design internal header
// See VFsmAxi4Wb.h for the primary calling header

#ifndef VERILATED_VFSMAXI4WB___024ROOT_H_
#define VERILATED_VFSMAXI4WB___024ROOT_H_  // guard

#include "verilated.h"


class VFsmAxi4Wb__Syms;

class alignas(VL_CACHE_LINE_BYTES) VFsmAxi4Wb___024root final : public VerilatedModule {
  public:

    // DESIGN SPECIFIC STATE
    VL_IN8(clk,0,0);
    VL_IN8(rst,0,0);
    VL_IN8(wb_start,0,0);
    VL_OUT8(wb_done,0,0);
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
    CData/*1:0*/ FsmAxi4Wb__DOT__state_r;
    CData/*1:0*/ FsmAxi4Wb__DOT__state_next;
    CData/*3:0*/ FsmAxi4Wb__DOT__beat_ctr_r;
    CData/*0:0*/ __VstlFirstIteration;
    CData/*0:0*/ __VicoFirstIteration;
    CData/*0:0*/ __Vtrigprevexpr___TOP__clk__0;
    CData/*0:0*/ __VactContinue;
    IData/*31:0*/ __VactIterCount;
    VL_IN64(wb_addr,63,0);
    VL_IN64(wb_word_0,63,0);
    VL_IN64(wb_word_1,63,0);
    VL_IN64(wb_word_2,63,0);
    VL_IN64(wb_word_3,63,0);
    VL_IN64(wb_word_4,63,0);
    VL_IN64(wb_word_5,63,0);
    VL_IN64(wb_word_6,63,0);
    VL_IN64(wb_word_7,63,0);
    VL_OUT64(aw_addr,63,0);
    VL_OUT64(w_data,63,0);
    QData/*63:0*/ FsmAxi4Wb__DOT__wb_addr_r;
    VlTriggerVec<1> __VstlTriggered;
    VlTriggerVec<1> __VicoTriggered;
    VlTriggerVec<1> __VactTriggered;
    VlTriggerVec<1> __VnbaTriggered;

    // INTERNAL VARIABLES
    VFsmAxi4Wb__Syms* const vlSymsp;

    // CONSTRUCTORS
    VFsmAxi4Wb___024root(VFsmAxi4Wb__Syms* symsp, const char* v__name);
    ~VFsmAxi4Wb___024root();
    VL_UNCOPYABLE(VFsmAxi4Wb___024root);

    // INTERNAL METHODS
    void __Vconfigure(bool first);
};


#endif  // guard
