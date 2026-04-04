// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Symbol table internal header
//
// Internal details; most calling programs do not need this header,
// unless using verilator public meta comments.

#ifndef VERILATED_VFSMAXI4WB__SYMS_H_
#define VERILATED_VFSMAXI4WB__SYMS_H_  // guard

#include "verilated.h"

// INCLUDE MODEL CLASS

#include "VFsmAxi4Wb.h"

// INCLUDE MODULE CLASSES
#include "VFsmAxi4Wb___024root.h"

// SYMS CLASS (contains all model state)
class alignas(VL_CACHE_LINE_BYTES)VFsmAxi4Wb__Syms final : public VerilatedSyms {
  public:
    // INTERNAL STATE
    VFsmAxi4Wb* const __Vm_modelp;
    VlDeleter __Vm_deleter;
    bool __Vm_didInit = false;

    // MODULE INSTANCE STATE
    VFsmAxi4Wb___024root           TOP;

    // CONSTRUCTORS
    VFsmAxi4Wb__Syms(VerilatedContext* contextp, const char* namep, VFsmAxi4Wb* modelp);
    ~VFsmAxi4Wb__Syms();

    // METHODS
    const char* name() { return TOP.name(); }
};

#endif  // guard
