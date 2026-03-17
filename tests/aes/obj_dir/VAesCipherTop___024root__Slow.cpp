// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Design implementation internals
// See VAesCipherTop.h for the primary calling header

#include "VAesCipherTop__pch.h"
#include "VAesCipherTop__Syms.h"
#include "VAesCipherTop___024root.h"

void VAesCipherTop___024root___ctor_var_reset(VAesCipherTop___024root* vlSelf);

VAesCipherTop___024root::VAesCipherTop___024root(VAesCipherTop__Syms* symsp, const char* v__name)
    : VerilatedModule{v__name}
    , vlSymsp{symsp}
 {
    // Reset structure values
    VAesCipherTop___024root___ctor_var_reset(this);
}

void VAesCipherTop___024root::__Vconfigure(bool first) {
    (void)first;  // Prevent unused variable warning
}

VAesCipherTop___024root::~VAesCipherTop___024root() {
}
