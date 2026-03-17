// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Symbol table internal header
//
// Internal details; most calling programs do not need this header,
// unless using verilator public meta comments.

#ifndef VERILATED_VAESCIPHERTOP__SYMS_H_
#define VERILATED_VAESCIPHERTOP__SYMS_H_  // guard

#include "verilated.h"

// INCLUDE MODEL CLASS

#include "VAesCipherTop.h"

// INCLUDE MODULE CLASSES
#include "VAesCipherTop___024root.h"

// SYMS CLASS (contains all model state)
class alignas(VL_CACHE_LINE_BYTES)VAesCipherTop__Syms final : public VerilatedSyms {
  public:
    // INTERNAL STATE
    VAesCipherTop* const __Vm_modelp;
    VlDeleter __Vm_deleter;
    bool __Vm_didInit = false;

    // MODULE INSTANCE STATE
    VAesCipherTop___024root        TOP;

    // CONSTRUCTORS
    VAesCipherTop__Syms(VerilatedContext* contextp, const char* namep, VAesCipherTop* modelp);
    ~VAesCipherTop__Syms();

    // METHODS
    const char* name() { return TOP.name(); }
};

#endif  // guard
