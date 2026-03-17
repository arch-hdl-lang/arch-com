// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Model implementation (design independent parts)

#include "VAesCipherTop__pch.h"

//============================================================
// Constructors

VAesCipherTop::VAesCipherTop(VerilatedContext* _vcontextp__, const char* _vcname__)
    : VerilatedModel{*_vcontextp__}
    , vlSymsp{new VAesCipherTop__Syms(contextp(), _vcname__, this)}
    , clk{vlSymsp->TOP.clk}
    , rst{vlSymsp->TOP.rst}
    , ld{vlSymsp->TOP.ld}
    , done{vlSymsp->TOP.done}
    , key{vlSymsp->TOP.key}
    , text_in{vlSymsp->TOP.text_in}
    , text_out{vlSymsp->TOP.text_out}
    , rootp{&(vlSymsp->TOP)}
{
    // Register model with the context
    contextp()->addModel(this);
}

VAesCipherTop::VAesCipherTop(const char* _vcname__)
    : VAesCipherTop(Verilated::threadContextp(), _vcname__)
{
}

//============================================================
// Destructor

VAesCipherTop::~VAesCipherTop() {
    delete vlSymsp;
}

//============================================================
// Evaluation function

#ifdef VL_DEBUG
void VAesCipherTop___024root___eval_debug_assertions(VAesCipherTop___024root* vlSelf);
#endif  // VL_DEBUG
void VAesCipherTop___024root___eval_static(VAesCipherTop___024root* vlSelf);
void VAesCipherTop___024root___eval_initial(VAesCipherTop___024root* vlSelf);
void VAesCipherTop___024root___eval_settle(VAesCipherTop___024root* vlSelf);
void VAesCipherTop___024root___eval(VAesCipherTop___024root* vlSelf);

void VAesCipherTop::eval_step() {
    VL_DEBUG_IF(VL_DBG_MSGF("+++++TOP Evaluate VAesCipherTop::eval_step\n"); );
#ifdef VL_DEBUG
    // Debug assertions
    VAesCipherTop___024root___eval_debug_assertions(&(vlSymsp->TOP));
#endif  // VL_DEBUG
    vlSymsp->__Vm_deleter.deleteAll();
    if (VL_UNLIKELY(!vlSymsp->__Vm_didInit)) {
        vlSymsp->__Vm_didInit = true;
        VL_DEBUG_IF(VL_DBG_MSGF("+ Initial\n"););
        VAesCipherTop___024root___eval_static(&(vlSymsp->TOP));
        VAesCipherTop___024root___eval_initial(&(vlSymsp->TOP));
        VAesCipherTop___024root___eval_settle(&(vlSymsp->TOP));
    }
    VL_DEBUG_IF(VL_DBG_MSGF("+ Eval\n"););
    VAesCipherTop___024root___eval(&(vlSymsp->TOP));
    // Evaluate cleanup
    Verilated::endOfEval(vlSymsp->__Vm_evalMsgQp);
}

//============================================================
// Events and timing
bool VAesCipherTop::eventsPending() { return false; }

uint64_t VAesCipherTop::nextTimeSlot() {
    VL_FATAL_MT(__FILE__, __LINE__, "", "No delays in the design");
    return 0;
}

//============================================================
// Utilities

const char* VAesCipherTop::name() const {
    return vlSymsp->name();
}

//============================================================
// Invoke final blocks

void VAesCipherTop___024root___eval_final(VAesCipherTop___024root* vlSelf);

VL_ATTR_COLD void VAesCipherTop::final() {
    VAesCipherTop___024root___eval_final(&(vlSymsp->TOP));
}

//============================================================
// Implementations of abstract methods from VerilatedModel

const char* VAesCipherTop::hierName() const { return vlSymsp->name(); }
const char* VAesCipherTop::modelName() const { return "VAesCipherTop"; }
unsigned VAesCipherTop::threads() const { return 1; }
void VAesCipherTop::prepareClone() const { contextp()->prepareClone(); }
void VAesCipherTop::atClone() const {
    contextp()->threadPoolpOnClone();
}
