// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Symbol table internal header
//
// Internal details; most calling programs do not need this header,
// unless using verilator public meta comments.

#ifndef VERILATED_VFSMAXI4FILL__SYMS_H_
#define VERILATED_VFSMAXI4FILL__SYMS_H_  // guard

#include "verilated.h"

// INCLUDE MODEL CLASS

#include "VFsmAxi4Fill.h"

// INCLUDE MODULE CLASSES
#include "VFsmAxi4Fill___024root.h"

// SYMS CLASS (contains all model state)
class alignas(VL_CACHE_LINE_BYTES)VFsmAxi4Fill__Syms final : public VerilatedSyms {
  public:
    // INTERNAL STATE
    VFsmAxi4Fill* const __Vm_modelp;
    VlDeleter __Vm_deleter;
    bool __Vm_didInit = false;

    // MODULE INSTANCE STATE
    VFsmAxi4Fill___024root         TOP;

    // CONSTRUCTORS
    VFsmAxi4Fill__Syms(VerilatedContext* contextp, const char* namep, VFsmAxi4Fill* modelp);
    ~VFsmAxi4Fill__Syms();

    // METHODS
    const char* name() { return TOP.name(); }
};

#endif  // guard
