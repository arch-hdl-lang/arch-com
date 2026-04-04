// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Symbol table implementation internals

#include "VFsmAxi4Wb__pch.h"
#include "VFsmAxi4Wb.h"
#include "VFsmAxi4Wb___024root.h"

// FUNCTIONS
VFsmAxi4Wb__Syms::~VFsmAxi4Wb__Syms()
{
}

VFsmAxi4Wb__Syms::VFsmAxi4Wb__Syms(VerilatedContext* contextp, const char* namep, VFsmAxi4Wb* modelp)
    : VerilatedSyms{contextp}
    // Setup internal state of the Syms class
    , __Vm_modelp{modelp}
    // Setup module instances
    , TOP{this, namep}
{
        // Check resources
        Verilated::stackCheck(39);
    // Configure time unit / time precision
    _vm_contextp__->timeunit(-12);
    _vm_contextp__->timeprecision(-12);
    // Setup each module's pointers to their submodules
    // Setup each module's pointer back to symbol table (for public functions)
    TOP.__Vconfigure(true);
}
