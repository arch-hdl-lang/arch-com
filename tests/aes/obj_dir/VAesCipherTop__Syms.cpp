// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Symbol table implementation internals

#include "VAesCipherTop__pch.h"
#include "VAesCipherTop.h"
#include "VAesCipherTop___024root.h"

// FUNCTIONS
VAesCipherTop__Syms::~VAesCipherTop__Syms()
{
}

VAesCipherTop__Syms::VAesCipherTop__Syms(VerilatedContext* contextp, const char* namep, VAesCipherTop* modelp)
    : VerilatedSyms{contextp}
    // Setup internal state of the Syms class
    , __Vm_modelp{modelp}
    // Setup module instances
    , TOP{this, namep}
{
        // Check resources
        Verilated::stackCheck(1108);
    // Configure time unit / time precision
    _vm_contextp__->timeunit(-12);
    _vm_contextp__->timeprecision(-12);
    // Setup each module's pointers to their submodules
    // Setup each module's pointer back to symbol table (for public functions)
    TOP.__Vconfigure(true);
}
