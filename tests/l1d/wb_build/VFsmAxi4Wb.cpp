// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Model implementation (design independent parts)

#include "VFsmAxi4Wb__pch.h"

//============================================================
// Constructors

VFsmAxi4Wb::VFsmAxi4Wb(VerilatedContext* _vcontextp__, const char* _vcname__)
    : VerilatedModel{*_vcontextp__}
    , vlSymsp{new VFsmAxi4Wb__Syms(contextp(), _vcname__, this)}
    , clk{vlSymsp->TOP.clk}
    , rst{vlSymsp->TOP.rst}
    , wb_start{vlSymsp->TOP.wb_start}
    , wb_done{vlSymsp->TOP.wb_done}
    , aw_valid{vlSymsp->TOP.aw_valid}
    , aw_ready{vlSymsp->TOP.aw_ready}
    , aw_id{vlSymsp->TOP.aw_id}
    , aw_len{vlSymsp->TOP.aw_len}
    , aw_size{vlSymsp->TOP.aw_size}
    , aw_burst{vlSymsp->TOP.aw_burst}
    , w_valid{vlSymsp->TOP.w_valid}
    , w_ready{vlSymsp->TOP.w_ready}
    , w_strb{vlSymsp->TOP.w_strb}
    , w_last{vlSymsp->TOP.w_last}
    , b_valid{vlSymsp->TOP.b_valid}
    , b_ready{vlSymsp->TOP.b_ready}
    , b_id{vlSymsp->TOP.b_id}
    , b_resp{vlSymsp->TOP.b_resp}
    , wb_addr{vlSymsp->TOP.wb_addr}
    , wb_word_0{vlSymsp->TOP.wb_word_0}
    , wb_word_1{vlSymsp->TOP.wb_word_1}
    , wb_word_2{vlSymsp->TOP.wb_word_2}
    , wb_word_3{vlSymsp->TOP.wb_word_3}
    , wb_word_4{vlSymsp->TOP.wb_word_4}
    , wb_word_5{vlSymsp->TOP.wb_word_5}
    , wb_word_6{vlSymsp->TOP.wb_word_6}
    , wb_word_7{vlSymsp->TOP.wb_word_7}
    , aw_addr{vlSymsp->TOP.aw_addr}
    , w_data{vlSymsp->TOP.w_data}
    , rootp{&(vlSymsp->TOP)}
{
    // Register model with the context
    contextp()->addModel(this);
}

VFsmAxi4Wb::VFsmAxi4Wb(const char* _vcname__)
    : VFsmAxi4Wb(Verilated::threadContextp(), _vcname__)
{
}

//============================================================
// Destructor

VFsmAxi4Wb::~VFsmAxi4Wb() {
    delete vlSymsp;
}

//============================================================
// Evaluation function

#ifdef VL_DEBUG
void VFsmAxi4Wb___024root___eval_debug_assertions(VFsmAxi4Wb___024root* vlSelf);
#endif  // VL_DEBUG
void VFsmAxi4Wb___024root___eval_static(VFsmAxi4Wb___024root* vlSelf);
void VFsmAxi4Wb___024root___eval_initial(VFsmAxi4Wb___024root* vlSelf);
void VFsmAxi4Wb___024root___eval_settle(VFsmAxi4Wb___024root* vlSelf);
void VFsmAxi4Wb___024root___eval(VFsmAxi4Wb___024root* vlSelf);

void VFsmAxi4Wb::eval_step() {
    VL_DEBUG_IF(VL_DBG_MSGF("+++++TOP Evaluate VFsmAxi4Wb::eval_step\n"); );
#ifdef VL_DEBUG
    // Debug assertions
    VFsmAxi4Wb___024root___eval_debug_assertions(&(vlSymsp->TOP));
#endif  // VL_DEBUG
    vlSymsp->__Vm_deleter.deleteAll();
    if (VL_UNLIKELY(!vlSymsp->__Vm_didInit)) {
        vlSymsp->__Vm_didInit = true;
        VL_DEBUG_IF(VL_DBG_MSGF("+ Initial\n"););
        VFsmAxi4Wb___024root___eval_static(&(vlSymsp->TOP));
        VFsmAxi4Wb___024root___eval_initial(&(vlSymsp->TOP));
        VFsmAxi4Wb___024root___eval_settle(&(vlSymsp->TOP));
    }
    VL_DEBUG_IF(VL_DBG_MSGF("+ Eval\n"););
    VFsmAxi4Wb___024root___eval(&(vlSymsp->TOP));
    // Evaluate cleanup
    Verilated::endOfEval(vlSymsp->__Vm_evalMsgQp);
}

//============================================================
// Events and timing
bool VFsmAxi4Wb::eventsPending() { return false; }

uint64_t VFsmAxi4Wb::nextTimeSlot() {
    VL_FATAL_MT(__FILE__, __LINE__, "", "No delays in the design");
    return 0;
}

//============================================================
// Utilities

const char* VFsmAxi4Wb::name() const {
    return vlSymsp->name();
}

//============================================================
// Invoke final blocks

void VFsmAxi4Wb___024root___eval_final(VFsmAxi4Wb___024root* vlSelf);

VL_ATTR_COLD void VFsmAxi4Wb::final() {
    VFsmAxi4Wb___024root___eval_final(&(vlSymsp->TOP));
}

//============================================================
// Implementations of abstract methods from VerilatedModel

const char* VFsmAxi4Wb::hierName() const { return vlSymsp->name(); }
const char* VFsmAxi4Wb::modelName() const { return "VFsmAxi4Wb"; }
unsigned VFsmAxi4Wb::threads() const { return 1; }
void VFsmAxi4Wb::prepareClone() const { contextp()->prepareClone(); }
void VFsmAxi4Wb::atClone() const {
    contextp()->threadPoolpOnClone();
}
