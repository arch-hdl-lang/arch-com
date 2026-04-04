// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Primary model header
//
// This header should be included by all source files instantiating the design.
// The class here is then constructed to instantiate the design.
// See the Verilator manual for examples.

#ifndef VERILATED_VFSMCACHECTRL_H_
#define VERILATED_VFSMCACHECTRL_H_  // guard

#include "verilated.h"

class VFsmCacheCtrl__Syms;
class VFsmCacheCtrl___024root;

// This class is the main interface to the Verilated model
class alignas(VL_CACHE_LINE_BYTES) VFsmCacheCtrl VL_NOT_FINAL : public VerilatedModel {
  private:
    // Symbol table holding complete model state (owned by this class)
    VFsmCacheCtrl__Syms* const vlSymsp;

  public:

    // CONSTEXPR CAPABILITIES
    // Verilated with --trace?
    static constexpr bool traceCapable = false;

    // PORTS
    // The application code writes and reads these signals to
    // propagate new values into/out from the Verilated model.
    VL_IN8(&clk,0,0);
    VL_IN8(&rst,0,0);
    VL_IN8(&req_valid,0,0);
    VL_OUT8(&req_ready,0,0);
    VL_IN8(&req_be,7,0);
    VL_IN8(&req_is_store,0,0);
    VL_OUT8(&resp_valid,0,0);
    VL_OUT8(&resp_error,0,0);
    VL_OUT8(&tag_rd_en_0,0,0);
    VL_OUT8(&tag_rd_addr_0,5,0);
    VL_OUT8(&tag_rd_en_1,0,0);
    VL_OUT8(&tag_rd_addr_1,5,0);
    VL_OUT8(&tag_rd_en_2,0,0);
    VL_OUT8(&tag_rd_addr_2,5,0);
    VL_OUT8(&tag_rd_en_3,0,0);
    VL_OUT8(&tag_rd_addr_3,5,0);
    VL_OUT8(&tag_rd_en_4,0,0);
    VL_OUT8(&tag_rd_addr_4,5,0);
    VL_OUT8(&tag_rd_en_5,0,0);
    VL_OUT8(&tag_rd_addr_5,5,0);
    VL_OUT8(&tag_rd_en_6,0,0);
    VL_OUT8(&tag_rd_addr_6,5,0);
    VL_OUT8(&tag_rd_en_7,0,0);
    VL_OUT8(&tag_rd_addr_7,5,0);
    VL_OUT8(&tag_wr_en_0,0,0);
    VL_OUT8(&tag_wr_addr_0,5,0);
    VL_OUT8(&tag_wr_en_1,0,0);
    VL_OUT8(&tag_wr_addr_1,5,0);
    VL_OUT8(&tag_wr_en_2,0,0);
    VL_OUT8(&tag_wr_addr_2,5,0);
    VL_OUT8(&tag_wr_en_3,0,0);
    VL_OUT8(&tag_wr_addr_3,5,0);
    VL_OUT8(&tag_wr_en_4,0,0);
    VL_OUT8(&tag_wr_addr_4,5,0);
    VL_OUT8(&tag_wr_en_5,0,0);
    VL_OUT8(&tag_wr_addr_5,5,0);
    VL_OUT8(&tag_wr_en_6,0,0);
    VL_OUT8(&tag_wr_addr_6,5,0);
    VL_OUT8(&tag_wr_en_7,0,0);
    VL_OUT8(&tag_wr_addr_7,5,0);
    VL_OUT8(&data_rd_en,0,0);
    VL_OUT8(&data_wr_en,0,0);
    VL_OUT8(&lru_rd_en,0,0);
    VL_OUT8(&lru_rd_addr,5,0);
    VL_IN8(&lru_rd_data,6,0);
    VL_OUT8(&lru_wr_en,0,0);
    VL_OUT8(&lru_wr_addr,5,0);
    VL_OUT8(&lru_wr_data,6,0);
    VL_OUT8(&lru_tree_in,6,0);
    VL_OUT8(&lru_access_way,2,0);
    VL_OUT8(&lru_access_en,0,0);
    VL_IN8(&lru_tree_out,6,0);
    VL_IN8(&lru_victim_way,2,0);
    VL_OUT8(&fill_start,0,0);
    VL_IN8(&fill_done,0,0);
    VL_OUT8(&wb_start,0,0);
    VL_IN8(&wb_done,0,0);
    VL_OUT16(&data_rd_addr,11,0);
    VL_OUT16(&data_wr_addr,11,0);
    VL_IN64(&req_vaddr,63,0);
    VL_IN64(&req_data,63,0);
    VL_OUT64(&resp_data,63,0);
    VL_IN64(&tag_rd_data_0,53,0);
    VL_IN64(&tag_rd_data_1,53,0);
    VL_IN64(&tag_rd_data_2,53,0);
    VL_IN64(&tag_rd_data_3,53,0);
    VL_IN64(&tag_rd_data_4,53,0);
    VL_IN64(&tag_rd_data_5,53,0);
    VL_IN64(&tag_rd_data_6,53,0);
    VL_IN64(&tag_rd_data_7,53,0);
    VL_OUT64(&tag_wr_data_0,53,0);
    VL_OUT64(&tag_wr_data_1,53,0);
    VL_OUT64(&tag_wr_data_2,53,0);
    VL_OUT64(&tag_wr_data_3,53,0);
    VL_OUT64(&tag_wr_data_4,53,0);
    VL_OUT64(&tag_wr_data_5,53,0);
    VL_OUT64(&tag_wr_data_6,53,0);
    VL_OUT64(&tag_wr_data_7,53,0);
    VL_IN64(&data_rd_data,63,0);
    VL_OUT64(&data_wr_data,63,0);
    VL_OUT64(&fill_addr,63,0);
    VL_IN64(&fill_word_0,63,0);
    VL_IN64(&fill_word_1,63,0);
    VL_IN64(&fill_word_2,63,0);
    VL_IN64(&fill_word_3,63,0);
    VL_IN64(&fill_word_4,63,0);
    VL_IN64(&fill_word_5,63,0);
    VL_IN64(&fill_word_6,63,0);
    VL_IN64(&fill_word_7,63,0);
    VL_OUT64(&wb_addr,63,0);
    VL_OUT64(&wb_word_0,63,0);
    VL_OUT64(&wb_word_1,63,0);
    VL_OUT64(&wb_word_2,63,0);
    VL_OUT64(&wb_word_3,63,0);
    VL_OUT64(&wb_word_4,63,0);
    VL_OUT64(&wb_word_5,63,0);
    VL_OUT64(&wb_word_6,63,0);
    VL_OUT64(&wb_word_7,63,0);

    // CELLS
    // Public to allow access to /* verilator public */ items.
    // Otherwise the application code can consider these internals.

    // Root instance pointer to allow access to model internals,
    // including inlined /* verilator public_flat_* */ items.
    VFsmCacheCtrl___024root* const rootp;

    // CONSTRUCTORS
    /// Construct the model; called by application code
    /// If contextp is null, then the model will use the default global context
    /// If name is "", then makes a wrapper with a
    /// single model invisible with respect to DPI scope names.
    explicit VFsmCacheCtrl(VerilatedContext* contextp, const char* name = "TOP");
    explicit VFsmCacheCtrl(const char* name = "TOP");
    /// Destroy the model; called (often implicitly) by application code
    virtual ~VFsmCacheCtrl();
  private:
    VL_UNCOPYABLE(VFsmCacheCtrl);  ///< Copying not allowed

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
