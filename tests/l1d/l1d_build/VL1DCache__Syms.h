// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Symbol table internal header
//
// Internal details; most calling programs do not need this header,
// unless using verilator public meta comments.

#ifndef VERILATED_VL1DCACHE__SYMS_H_
#define VERILATED_VL1DCACHE__SYMS_H_  // guard

#include "verilated.h"

// INCLUDE MODEL CLASS

#include "VL1DCache.h"

// INCLUDE MODULE CLASSES
#include "VL1DCache___024root.h"

// SYMS CLASS (contains all model state)
class alignas(VL_CACHE_LINE_BYTES)VL1DCache__Syms final : public VerilatedSyms {
  public:
    // INTERNAL STATE
    VL1DCache* const __Vm_modelp;
    VlDeleter __Vm_deleter;
    bool __Vm_didInit = false;

    // MODULE INSTANCE STATE
    VL1DCache___024root            TOP;

    // CONSTRUCTORS
    VL1DCache__Syms(VerilatedContext* contextp, const char* namep, VL1DCache* modelp);
    ~VL1DCache__Syms();

    // METHODS
    const char* name() { return TOP.name(); }
};

#endif  // guard
