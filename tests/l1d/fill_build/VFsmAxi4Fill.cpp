// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Model implementation (design independent parts)

#include "VFsmAxi4Fill__pch.h"

//============================================================
// Constructors

VFsmAxi4Fill::VFsmAxi4Fill(VerilatedContext* _vcontextp__, const char* _vcname__)
    : VerilatedModel{*_vcontextp__}
    , vlSymsp{new VFsmAxi4Fill__Syms(contextp(), _vcname__, this)}
    , clk{vlSymsp->TOP.clk}
    , rst{vlSymsp->TOP.rst}
    , fill_start{vlSymsp->TOP.fill_start}
    , fill_done{vlSymsp->TOP.fill_done}
    , ar_valid{vlSymsp->TOP.ar_valid}
    , ar_ready{vlSymsp->TOP.ar_ready}
    , ar_id{vlSymsp->TOP.ar_id}
    , ar_len{vlSymsp->TOP.ar_len}
    , ar_size{vlSymsp->TOP.ar_size}
    , ar_burst{vlSymsp->TOP.ar_burst}
    , r_valid{vlSymsp->TOP.r_valid}
    , r_ready{vlSymsp->TOP.r_ready}
    , r_id{vlSymsp->TOP.r_id}
    , r_resp{vlSymsp->TOP.r_resp}
    , r_last{vlSymsp->TOP.r_last}
    , fill_addr{vlSymsp->TOP.fill_addr}
    , fill_word_0{vlSymsp->TOP.fill_word_0}
    , fill_word_1{vlSymsp->TOP.fill_word_1}
    , fill_word_2{vlSymsp->TOP.fill_word_2}
    , fill_word_3{vlSymsp->TOP.fill_word_3}
    , fill_word_4{vlSymsp->TOP.fill_word_4}
    , fill_word_5{vlSymsp->TOP.fill_word_5}
    , fill_word_6{vlSymsp->TOP.fill_word_6}
    , fill_word_7{vlSymsp->TOP.fill_word_7}
    , ar_addr{vlSymsp->TOP.ar_addr}
    , r_data{vlSymsp->TOP.r_data}
    , rootp{&(vlSymsp->TOP)}
{
    // Register model with the context
    contextp()->addModel(this);
}

VFsmAxi4Fill::VFsmAxi4Fill(const char* _vcname__)
    : VFsmAxi4Fill(Verilated::threadContextp(), _vcname__)
{
}

//============================================================
// Destructor

VFsmAxi4Fill::~VFsmAxi4Fill() {
    delete vlSymsp;
}

//============================================================
// Evaluation function

#ifdef VL_DEBUG
void VFsmAxi4Fill___024root___eval_debug_assertions(VFsmAxi4Fill___024root* vlSelf);
#endif  // VL_DEBUG
void VFsmAxi4Fill___024root___eval_static(VFsmAxi4Fill___024root* vlSelf);
void VFsmAxi4Fill___024root___eval_initial(VFsmAxi4Fill___024root* vlSelf);
void VFsmAxi4Fill___024root___eval_settle(VFsmAxi4Fill___024root* vlSelf);
void VFsmAxi4Fill___024root___eval(VFsmAxi4Fill___024root* vlSelf);

void VFsmAxi4Fill::eval_step() {
    VL_DEBUG_IF(VL_DBG_MSGF("+++++TOP Evaluate VFsmAxi4Fill::eval_step\n"); );
#ifdef VL_DEBUG
    // Debug assertions
    VFsmAxi4Fill___024root___eval_debug_assertions(&(vlSymsp->TOP));
#endif  // VL_DEBUG
    vlSymsp->__Vm_deleter.deleteAll();
    if (VL_UNLIKELY(!vlSymsp->__Vm_didInit)) {
        vlSymsp->__Vm_didInit = true;
        VL_DEBUG_IF(VL_DBG_MSGF("+ Initial\n"););
        VFsmAxi4Fill___024root___eval_static(&(vlSymsp->TOP));
        VFsmAxi4Fill___024root___eval_initial(&(vlSymsp->TOP));
        VFsmAxi4Fill___024root___eval_settle(&(vlSymsp->TOP));
    }
    VL_DEBUG_IF(VL_DBG_MSGF("+ Eval\n"););
    VFsmAxi4Fill___024root___eval(&(vlSymsp->TOP));
    // Evaluate cleanup
    Verilated::endOfEval(vlSymsp->__Vm_evalMsgQp);
}

//============================================================
// Events and timing
bool VFsmAxi4Fill::eventsPending() { return false; }

uint64_t VFsmAxi4Fill::nextTimeSlot() {
    VL_FATAL_MT(__FILE__, __LINE__, "", "No delays in the design");
    return 0;
}

//============================================================
// Utilities

const char* VFsmAxi4Fill::name() const {
    return vlSymsp->name();
}

//============================================================
// Invoke final blocks

void VFsmAxi4Fill___024root___eval_final(VFsmAxi4Fill___024root* vlSelf);

VL_ATTR_COLD void VFsmAxi4Fill::final() {
    VFsmAxi4Fill___024root___eval_final(&(vlSymsp->TOP));
}

//============================================================
// Implementations of abstract methods from VerilatedModel

const char* VFsmAxi4Fill::hierName() const { return vlSymsp->name(); }
const char* VFsmAxi4Fill::modelName() const { return "VFsmAxi4Fill"; }
unsigned VFsmAxi4Fill::threads() const { return 1; }
void VFsmAxi4Fill::prepareClone() const { contextp()->prepareClone(); }
void VFsmAxi4Fill::atClone() const {
    contextp()->threadPoolpOnClone();
}
