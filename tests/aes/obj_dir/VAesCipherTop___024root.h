// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Design internal header
// See VAesCipherTop.h for the primary calling header

#ifndef VERILATED_VAESCIPHERTOP___024ROOT_H_
#define VERILATED_VAESCIPHERTOP___024ROOT_H_  // guard

#include "verilated.h"


class VAesCipherTop__Syms;

class alignas(VL_CACHE_LINE_BYTES) VAesCipherTop___024root final : public VerilatedModule {
  public:

    // DESIGN SPECIFIC STATE
    // Anonymous structures to workaround compiler member-count bugs
    struct {
        VL_IN8(clk,0,0);
        VL_IN8(rst,0,0);
        VL_IN8(ld,0,0);
        VL_OUT8(done,0,0);
        CData/*3:0*/ AesCipherTop__DOT__dcnt;
        CData/*0:0*/ AesCipherTop__DOT__ld_r;
        CData/*0:0*/ AesCipherTop__DOT__done_r;
        CData/*7:0*/ AesCipherTop__DOT__sa00;
        CData/*7:0*/ AesCipherTop__DOT__sa01;
        CData/*7:0*/ AesCipherTop__DOT__sa02;
        CData/*7:0*/ AesCipherTop__DOT__sa03;
        CData/*7:0*/ AesCipherTop__DOT__sa10;
        CData/*7:0*/ AesCipherTop__DOT__sa11;
        CData/*7:0*/ AesCipherTop__DOT__sa12;
        CData/*7:0*/ AesCipherTop__DOT__sa13;
        CData/*7:0*/ AesCipherTop__DOT__sa20;
        CData/*7:0*/ AesCipherTop__DOT__sa21;
        CData/*7:0*/ AesCipherTop__DOT__sa22;
        CData/*7:0*/ AesCipherTop__DOT__sa23;
        CData/*7:0*/ AesCipherTop__DOT__sa30;
        CData/*7:0*/ AesCipherTop__DOT__sa31;
        CData/*7:0*/ AesCipherTop__DOT__sa32;
        CData/*7:0*/ AesCipherTop__DOT__sa33;
        CData/*7:0*/ AesCipherTop__DOT__sa00_sr;
        CData/*7:0*/ AesCipherTop__DOT__sa01_sr;
        CData/*7:0*/ AesCipherTop__DOT__sa02_sr;
        CData/*7:0*/ AesCipherTop__DOT__sa03_sr;
        CData/*7:0*/ AesCipherTop__DOT__sa10_sr;
        CData/*7:0*/ AesCipherTop__DOT__sa11_sr;
        CData/*7:0*/ AesCipherTop__DOT__sa12_sr;
        CData/*7:0*/ AesCipherTop__DOT__sa13_sr;
        CData/*7:0*/ AesCipherTop__DOT__sa20_sr;
        CData/*7:0*/ AesCipherTop__DOT__sa21_sr;
        CData/*7:0*/ AesCipherTop__DOT__sa22_sr;
        CData/*7:0*/ AesCipherTop__DOT__sa23_sr;
        CData/*7:0*/ AesCipherTop__DOT__sa31_sr;
        CData/*7:0*/ AesCipherTop__DOT__sa32_sr;
        CData/*7:0*/ AesCipherTop__DOT__sa00_mc;
        CData/*7:0*/ AesCipherTop__DOT__sa10_mc;
        CData/*7:0*/ AesCipherTop__DOT__sa20_mc;
        CData/*7:0*/ AesCipherTop__DOT__sa30_mc;
        CData/*7:0*/ AesCipherTop__DOT__sa01_mc;
        CData/*7:0*/ AesCipherTop__DOT__sa11_mc;
        CData/*7:0*/ AesCipherTop__DOT__sa21_mc;
        CData/*7:0*/ AesCipherTop__DOT__sa31_mc;
        CData/*7:0*/ AesCipherTop__DOT__sa02_mc;
        CData/*7:0*/ AesCipherTop__DOT__sa12_mc;
        CData/*7:0*/ AesCipherTop__DOT__sa22_mc;
        CData/*7:0*/ AesCipherTop__DOT__sa32_mc;
        CData/*7:0*/ AesCipherTop__DOT__sa03_mc;
        CData/*7:0*/ AesCipherTop__DOT__sa13_mc;
        CData/*7:0*/ AesCipherTop__DOT__sa23_mc;
        CData/*7:0*/ AesCipherTop__DOT__sa33_mc;
        CData/*7:0*/ AesCipherTop__DOT__sa00_fark;
        CData/*7:0*/ AesCipherTop__DOT__sa10_fark;
        CData/*7:0*/ AesCipherTop__DOT__sa20_fark;
        CData/*7:0*/ AesCipherTop__DOT__sa30_fark;
        CData/*7:0*/ AesCipherTop__DOT__sa01_fark;
        CData/*7:0*/ AesCipherTop__DOT__sa11_fark;
        CData/*7:0*/ AesCipherTop__DOT__sa21_fark;
        CData/*7:0*/ AesCipherTop__DOT__sa31_fark;
        CData/*7:0*/ AesCipherTop__DOT__sa02_fark;
        CData/*7:0*/ AesCipherTop__DOT__sa12_fark;
        CData/*7:0*/ AesCipherTop__DOT__sa22_fark;
    };
    struct {
        CData/*7:0*/ AesCipherTop__DOT__sa32_fark;
        CData/*7:0*/ AesCipherTop__DOT__sa03_fark;
        CData/*7:0*/ AesCipherTop__DOT__sa13_fark;
        CData/*7:0*/ AesCipherTop__DOT__sa23_fark;
        CData/*7:0*/ AesCipherTop__DOT__sa33_fark;
        CData/*3:0*/ AesCipherTop__DOT__key_exp__DOT__rcnt;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__AesSbox__11__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__AesSbox__15__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__16__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__16__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__16__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__17__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__17__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__17__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__18__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__18__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__18__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__19__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__19__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__19__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__20__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__20__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__20__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__21__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__21__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__21__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__22__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__22__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__22__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__23__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__23__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__23__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__24__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__24__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__24__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__25__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__25__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__25__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__26__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__26__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__26__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__27__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__27__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__27__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__28__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__28__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__28__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__29__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__29__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__29__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__30__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__30__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__30__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__31__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__31__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__31__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__32__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__32__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__32__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__33__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__33__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__33__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__34__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__34__a;
    };
    struct {
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__34__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__35__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__35__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__35__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__36__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__36__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__36__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__37__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__37__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__37__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__38__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__38__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__38__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__39__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__39__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__39__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__40__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__40__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__40__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__41__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__41__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__41__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__42__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__42__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__42__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__43__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__43__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__43__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__44__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__44__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__44__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__45__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__45__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__45__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__46__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__46__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__46__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__47__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__47__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__Xtime__47__shifted;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__key_exp__DOT__AesSbox__48__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__key_exp__DOT__AesSbox__48__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__key_exp__DOT__AesSbox__49__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__key_exp__DOT__AesSbox__49__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__key_exp__DOT__AesSbox__50__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__key_exp__DOT__AesSbox__50__a;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__key_exp__DOT__AesSbox__51__Vfuncout;
        CData/*7:0*/ __Vfunc_AesCipherTop__DOT__key_exp__DOT__AesSbox__51__a;
        CData/*0:0*/ __VstlFirstIteration;
        CData/*0:0*/ __Vtrigprevexpr___TOP__clk__0;
        CData/*0:0*/ __VactContinue;
        VL_INW(key,127,0,4);
        VL_INW(text_in,127,0,4);
        VL_OUTW(text_out,127,0,4);
        VlWide<4>/*127:0*/ AesCipherTop__DOT__text_in_r;
        VlWide<4>/*127:0*/ AesCipherTop__DOT__text_out_r;
        IData/*31:0*/ AesCipherTop__DOT__key_exp__DOT__w0;
        IData/*31:0*/ AesCipherTop__DOT__key_exp__DOT__w1;
        IData/*31:0*/ AesCipherTop__DOT__key_exp__DOT__w2;
        IData/*31:0*/ AesCipherTop__DOT__key_exp__DOT__w3;
        IData/*31:0*/ AesCipherTop__DOT__key_exp__DOT__subword;
        IData/*31:0*/ AesCipherTop__DOT__key_exp__DOT__nw0;
        IData/*31:0*/ AesCipherTop__DOT__key_exp__DOT__nw1;
        IData/*31:0*/ AesCipherTop__DOT__key_exp__DOT__nw2;
    };
    struct {
        IData/*31:0*/ AesCipherTop__DOT__key_exp__DOT__nw3;
        IData/*31:0*/ __VactIterCount;
    };
    VlTriggerVec<1> __VstlTriggered;
    VlTriggerVec<1> __VactTriggered;
    VlTriggerVec<1> __VnbaTriggered;

    // INTERNAL VARIABLES
    VAesCipherTop__Syms* const vlSymsp;

    // CONSTRUCTORS
    VAesCipherTop___024root(VAesCipherTop__Syms* symsp, const char* v__name);
    ~VAesCipherTop___024root();
    VL_UNCOPYABLE(VAesCipherTop___024root);

    // INTERNAL METHODS
    void __Vconfigure(bool first);
};


#endif  // guard
