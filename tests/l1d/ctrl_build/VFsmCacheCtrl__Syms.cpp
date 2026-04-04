// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Symbol table implementation internals

#include "VFsmCacheCtrl__pch.h"
#include "VFsmCacheCtrl.h"
#include "VFsmCacheCtrl___024root.h"

// FUNCTIONS
VFsmCacheCtrl__Syms::~VFsmCacheCtrl__Syms()
{
}

VFsmCacheCtrl__Syms::VFsmCacheCtrl__Syms(VerilatedContext* contextp, const char* namep, VFsmCacheCtrl* modelp)
    : VerilatedSyms{contextp}
    // Setup internal state of the Syms class
    , __Vm_modelp{modelp}
    // Setup module instances
    , TOP{this, namep}
{
        // Check resources
        Verilated::stackCheck(27);
    // Configure time unit / time precision
    _vm_contextp__->timeunit(-12);
    _vm_contextp__->timeprecision(-12);
    // Setup each module's pointers to their submodules
    // Setup each module's pointer back to symbol table (for public functions)
    TOP.__Vconfigure(true);
}
