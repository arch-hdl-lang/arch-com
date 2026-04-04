// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Primary model header
//
// This header should be included by all source files instantiating the design.
// The class here is then constructed to instantiate the design.
// See the Verilator manual for examples.

#ifndef VERILATED_VFSMAXI4WB_H_
#define VERILATED_VFSMAXI4WB_H_  // guard

#include "verilated.h"

class VFsmAxi4Wb__Syms;
class VFsmAxi4Wb___024root;

// This class is the main interface to the Verilated model
class alignas(VL_CACHE_LINE_BYTES) VFsmAxi4Wb VL_NOT_FINAL : public VerilatedModel {
  private:
    // Symbol table holding complete model state (owned by this class)
    VFsmAxi4Wb__Syms* const vlSymsp;

  public:

    // CONSTEXPR CAPABILITIES
    // Verilated with --trace?
    static constexpr bool traceCapable = false;

    // PORTS
    // The application code writes and reads these signals to
    // propagate new values into/out from the Verilated model.
    VL_IN8(&clk,0,0);
    VL_IN8(&rst,0,0);
    VL_IN8(&wb_start,0,0);
    VL_OUT8(&wb_done,0,0);
    VL_OUT8(&aw_valid,0,0);
    VL_IN8(&aw_ready,0,0);
    VL_OUT8(&aw_id,3,0);
    VL_OUT8(&aw_len,7,0);
    VL_OUT8(&aw_size,2,0);
    VL_OUT8(&aw_burst,1,0);
    VL_OUT8(&w_valid,0,0);
    VL_IN8(&w_ready,0,0);
    VL_OUT8(&w_strb,7,0);
    VL_OUT8(&w_last,0,0);
    VL_IN8(&b_valid,0,0);
    VL_OUT8(&b_ready,0,0);
    VL_IN8(&b_id,3,0);
    VL_IN8(&b_resp,1,0);
    VL_IN64(&wb_addr,63,0);
    VL_IN64(&wb_word_0,63,0);
    VL_IN64(&wb_word_1,63,0);
    VL_IN64(&wb_word_2,63,0);
    VL_IN64(&wb_word_3,63,0);
    VL_IN64(&wb_word_4,63,0);
    VL_IN64(&wb_word_5,63,0);
    VL_IN64(&wb_word_6,63,0);
    VL_IN64(&wb_word_7,63,0);
    VL_OUT64(&aw_addr,63,0);
    VL_OUT64(&w_data,63,0);

    // CELLS
    // Public to allow access to /* verilator public */ items.
    // Otherwise the application code can consider these internals.

    // Root instance pointer to allow access to model internals,
    // including inlined /* verilator public_flat_* */ items.
    VFsmAxi4Wb___024root* const rootp;

    // CONSTRUCTORS
    /// Construct the model; called by application code
    /// If contextp is null, then the model will use the default global context
    /// If name is "", then makes a wrapper with a
    /// single model invisible with respect to DPI scope names.
    explicit VFsmAxi4Wb(VerilatedContext* contextp, const char* name = "TOP");
    explicit VFsmAxi4Wb(const char* name = "TOP");
    /// Destroy the model; called (often implicitly) by application code
    virtual ~VFsmAxi4Wb();
  private:
    VL_UNCOPYABLE(VFsmAxi4Wb);  ///< Copying not allowed

  public:
    // API METHODS
    /// Evaluate the model.  Application must call when inputs change.
    void eval() { eval_step(); }
    /// Evaluate when calling multiple units/models per time step.
    void eval_step();
    /// Evaluate at end of a timestep for tracing, when using eval_step().
    /// Application must call after all eval() and before time changes.
    void eval_end_step() {}
    /// Simulation complete, run final blocks.  Application must call on completion.
    void final();
    /// Are there scheduled events to handle?
    bool eventsPending();
    /// Returns time at next time slot. Aborts if !eventsPending()
    uint64_t nextTimeSlot();
    /// Trace signals in the model; called by application code
    void trace(VerilatedTraceBaseC* tfp, int levels, int options = 0) { contextp()->trace(tfp, levels, options); }
    /// Retrieve name of this model instance (as passed to constructor).
    const char* name() const;

    // Abstract methods from VerilatedModel
    const char* hierName() const override final;
    const char* modelName() const override final;
    unsigned threads() const override final;
    /// Prepare for cloning the model at the process level (e.g. fork in Linux)
    /// Release necessary resources. Called before cloning.
    void prepareClone() const;
    /// Re-init after cloning the model at the process level (e.g. fork in Linux)
    /// Re-allocate necessary resources. Called after cloning.
    void atClone() const;
  private:
    // Internal functions - trace registration
    void traceBaseModel(VerilatedTraceBaseC* tfp, int levels, int options);
};

#endif  // guard
