// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Symbol table internal header
//
// Internal details; most calling programs do not need this header,
// unless using verilator public meta comments.

#ifndef VERILATED_VFSMCACHECTRL__SYMS_H_
#define VERILATED_VFSMCACHECTRL__SYMS_H_  // guard

#include "verilated.h"

// INCLUDE MODEL CLASS

#include "VFsmCacheCtrl.h"

// INCLUDE MODULE CLASSES
#include "VFsmCacheCtrl___024root.h"

// SYMS CLASS (contains all model state)
class alignas(VL_CACHE_LINE_BYTES)VFsmCacheCtrl__Syms final : public VerilatedSyms {
  public:
    // INTERNAL STATE
    VFsmCacheCtrl* const __Vm_modelp;
    VlDeleter __Vm_deleter;
    bool __Vm_didInit = false;

    // MODULE INSTANCE STATE
    VFsmCacheCtrl___024root        TOP;

    // CONSTRUCTORS
    VFsmCacheCtrl__Syms(VerilatedContext* contextp, const char* namep, VFsmCacheCtrl* modelp);
    ~VFsmCacheCtrl__Syms();

    // METHODS
    const char* name() { return TOP.name(); }
};

#endif  // guard
