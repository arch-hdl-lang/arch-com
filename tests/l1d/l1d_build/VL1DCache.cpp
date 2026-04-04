// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Model implementation (design independent parts)

#include "VL1DCache__pch.h"

//============================================================
// Constructors

VL1DCache::VL1DCache(VerilatedContext* _vcontextp__, const char* _vcname__)
    : VerilatedModel{*_vcontextp__}
    , vlSymsp{new VL1DCache__Syms(contextp(), _vcname__, this)}
    , clk{vlSymsp->TOP.clk}
    , rst{vlSymsp->TOP.rst}
    , req_valid{vlSymsp->TOP.req_valid}
    , req_ready{vlSymsp->TOP.req_ready}
    , req_be{vlSymsp->TOP.req_be}
    , req_is_store{vlSymsp->TOP.req_is_store}
    , resp_valid{vlSymsp->TOP.resp_valid}
    , resp_error{vlSymsp->TOP.resp_error}
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
    , req_vaddr{vlSymsp->TOP.req_vaddr}
    , req_data{vlSymsp->TOP.req_data}
    , resp_data{vlSymsp->TOP.resp_data}
    , ar_addr{vlSymsp->TOP.ar_addr}
    , r_data{vlSymsp->TOP.r_data}
    , aw_addr{vlSymsp->TOP.aw_addr}
    , w_data{vlSymsp->TOP.w_data}
    , rootp{&(vlSymsp->TOP)}
{
    // Register model with the context
    contextp()->addModel(this);
}

VL1DCache::VL1DCache(const char* _vcname__)
    : VL1DCache(Verilated::threadContextp(), _vcname__)
{
}

//============================================================
// Destructor

VL1DCache::~VL1DCache() {
    delete vlSymsp;
}

//============================================================
// Evaluation function

#ifdef VL_DEBUG
void VL1DCache___024root___eval_debug_assertions(VL1DCache___024root* vlSelf);
#endif  // VL_DEBUG
void VL1DCache___024root___eval_static(VL1DCache___024root* vlSelf);
void VL1DCache___024root___eval_initial(VL1DCache___024root* vlSelf);
void VL1DCache___024root___eval_settle(VL1DCache___024root* vlSelf);
void VL1DCache___024root___eval(VL1DCache___024root* vlSelf);

void VL1DCache::eval_step() {
    VL_DEBUG_IF(VL_DBG_MSGF("+++++TOP Evaluate VL1DCache::eval_step\n"); );
#ifdef VL_DEBUG
    // Debug assertions
    VL1DCache___024root___eval_debug_assertions(&(vlSymsp->TOP));
#endif  // VL_DEBUG
    vlSymsp->__Vm_deleter.deleteAll();
    if (VL_UNLIKELY(!vlSymsp->__Vm_didInit)) {
        vlSymsp->__Vm_didInit = true;
        VL_DEBUG_IF(VL_DBG_MSGF("+ Initial\n"););
        VL1DCache___024root___eval_static(&(vlSymsp->TOP));
        VL1DCache___024root___eval_initial(&(vlSymsp->TOP));
        VL1DCache___024root___eval_settle(&(vlSymsp->TOP));
    }
    VL_DEBUG_IF(VL_DBG_MSGF("+ Eval\n"););
    VL1DCache___024root___eval(&(vlSymsp->TOP));
    // Evaluate cleanup
    Verilated::endOfEval(vlSymsp->__Vm_evalMsgQp);
}

//============================================================
// Events and timing
bool VL1DCache::eventsPending() { return false; }

uint64_t VL1DCache::nextTimeSlot() {
    VL_FATAL_MT(__FILE__, __LINE__, "", "No delays in the design");
    return 0;
}

//============================================================
// Utilities

const char* VL1DCache::name() const {
    return vlSymsp->name();
}

//============================================================
// Invoke final blocks

void VL1DCache___024root___eval_final(VL1DCache___024root* vlSelf);

VL_ATTR_COLD void VL1DCache::final() {
    VL1DCache___024root___eval_final(&(vlSymsp->TOP));
}

//============================================================
// Implementations of abstract methods from VerilatedModel

const char* VL1DCache::hierName() const { return vlSymsp->name(); }
const char* VL1DCache::modelName() const { return "VL1DCache"; }
unsigned VL1DCache::threads() const { return 1; }
void VL1DCache::prepareClone() const { contextp()->prepareClone(); }
void VL1DCache::atClone() const {
    contextp()->threadPoolpOnClone();
}
