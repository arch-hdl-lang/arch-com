// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Design internal header
// See VFsmAxi4Fill.h for the primary calling header

#ifndef VERILATED_VFSMAXI4FILL___024ROOT_H_
#define VERILATED_VFSMAXI4FILL___024ROOT_H_  // guard

#include "verilated.h"


class VFsmAxi4Fill__Syms;

class alignas(VL_CACHE_LINE_BYTES) VFsmAxi4Fill___024root final : public VerilatedModule {
  public:

    // DESIGN SPECIFIC STATE
    VL_IN8(clk,0,0);
    VL_IN8(rst,0,0);
    VL_IN8(fill_start,0,0);
    VL_OUT8(fill_done,0,0);
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
    CData/*1:0*/ FsmAxi4Fill__DOT__state_r;
    CData/*1:0*/ FsmAxi4Fill__DOT__state_next;
    CData/*3:0*/ FsmAxi4Fill__DOT__beat_ctr_r;
    CData/*0:0*/ __VstlFirstIteration;
    CData/*0:0*/ __VicoFirstIteration;
    CData/*0:0*/ __Vtrigprevexpr___TOP__clk__0;
    CData/*0:0*/ __VactContinue;
    IData/*31:0*/ __VactIterCount;
    VL_IN64(fill_addr,63,0);
    VL_OUT64(fill_word_0,63,0);
    VL_OUT64(fill_word_1,63,0);
    VL_OUT64(fill_word_2,63,0);
    VL_OUT64(fill_word_3,63,0);
    VL_OUT64(fill_word_4,63,0);
    VL_OUT64(fill_word_5,63,0);
    VL_OUT64(fill_word_6,63,0);
    VL_OUT64(fill_word_7,63,0);
    VL_OUT64(ar_addr,63,0);
    VL_IN64(r_data,63,0);
    QData/*63:0*/ FsmAxi4Fill__DOT__fill_addr_r;
    VlTriggerVec<1> __VstlTriggered;
    VlTriggerVec<1> __VicoTriggered;
    VlTriggerVec<1> __VactTriggered;
    VlTriggerVec<1> __VnbaTriggered;

    // INTERNAL VARIABLES
    VFsmAxi4Fill__Syms* const vlSymsp;

    // CONSTRUCTORS
    VFsmAxi4Fill___024root(VFsmAxi4Fill__Syms* symsp, const char* v__name);
    ~VFsmAxi4Fill___024root();
    VL_UNCOPYABLE(VFsmAxi4Fill___024root);

    // INTERNAL METHODS
    void __Vconfigure(bool first);
};


#endif  // guard
