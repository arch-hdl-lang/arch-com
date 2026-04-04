// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Model implementation (design independent parts)

#include "VFsmCacheCtrl__pch.h"

//============================================================
// Constructors

VFsmCacheCtrl::VFsmCacheCtrl(VerilatedContext* _vcontextp__, const char* _vcname__)
    : VerilatedModel{*_vcontextp__}
    , vlSymsp{new VFsmCacheCtrl__Syms(contextp(), _vcname__, this)}
    , clk{vlSymsp->TOP.clk}
    , rst{vlSymsp->TOP.rst}
    , req_valid{vlSymsp->TOP.req_valid}
    , req_ready{vlSymsp->TOP.req_ready}
    , req_be{vlSymsp->TOP.req_be}
    , req_is_store{vlSymsp->TOP.req_is_store}
    , resp_valid{vlSymsp->TOP.resp_valid}
    , resp_error{vlSymsp->TOP.resp_error}
    , tag_rd_en_0{vlSymsp->TOP.tag_rd_en_0}
    , tag_rd_addr_0{vlSymsp->TOP.tag_rd_addr_0}
    , tag_rd_en_1{vlSymsp->TOP.tag_rd_en_1}
    , tag_rd_addr_1{vlSymsp->TOP.tag_rd_addr_1}
    , tag_rd_en_2{vlSymsp->TOP.tag_rd_en_2}
    , tag_rd_addr_2{vlSymsp->TOP.tag_rd_addr_2}
    , tag_rd_en_3{vlSymsp->TOP.tag_rd_en_3}
    , tag_rd_addr_3{vlSymsp->TOP.tag_rd_addr_3}
    , tag_rd_en_4{vlSymsp->TOP.tag_rd_en_4}
    , tag_rd_addr_4{vlSymsp->TOP.tag_rd_addr_4}
    , tag_rd_en_5{vlSymsp->TOP.tag_rd_en_5}
    , tag_rd_addr_5{vlSymsp->TOP.tag_rd_addr_5}
    , tag_rd_en_6{vlSymsp->TOP.tag_rd_en_6}
    , tag_rd_addr_6{vlSymsp->TOP.tag_rd_addr_6}
    , tag_rd_en_7{vlSymsp->TOP.tag_rd_en_7}
    , tag_rd_addr_7{vlSymsp->TOP.tag_rd_addr_7}
    , tag_wr_en_0{vlSymsp->TOP.tag_wr_en_0}
    , tag_wr_addr_0{vlSymsp->TOP.tag_wr_addr_0}
    , tag_wr_en_1{vlSymsp->TOP.tag_wr_en_1}
    , tag_wr_addr_1{vlSymsp->TOP.tag_wr_addr_1}
    , tag_wr_en_2{vlSymsp->TOP.tag_wr_en_2}
    , tag_wr_addr_2{vlSymsp->TOP.tag_wr_addr_2}
    , tag_wr_en_3{vlSymsp->TOP.tag_wr_en_3}
    , tag_wr_addr_3{vlSymsp->TOP.tag_wr_addr_3}
    , tag_wr_en_4{vlSymsp->TOP.tag_wr_en_4}
    , tag_wr_addr_4{vlSymsp->TOP.tag_wr_addr_4}
    , tag_wr_en_5{vlSymsp->TOP.tag_wr_en_5}
    , tag_wr_addr_5{vlSymsp->TOP.tag_wr_addr_5}
    , tag_wr_en_6{vlSymsp->TOP.tag_wr_en_6}
    , tag_wr_addr_6{vlSymsp->TOP.tag_wr_addr_6}
    , tag_wr_en_7{vlSymsp->TOP.tag_wr_en_7}
    , tag_wr_addr_7{vlSymsp->TOP.tag_wr_addr_7}
    , data_rd_en{vlSymsp->TOP.data_rd_en}
    , data_wr_en{vlSymsp->TOP.data_wr_en}
    , lru_rd_en{vlSymsp->TOP.lru_rd_en}
    , lru_rd_addr{vlSymsp->TOP.lru_rd_addr}
    , lru_rd_data{vlSymsp->TOP.lru_rd_data}
    , lru_wr_en{vlSymsp->TOP.lru_wr_en}
    , lru_wr_addr{vlSymsp->TOP.lru_wr_addr}
    , lru_wr_data{vlSymsp->TOP.lru_wr_data}
    , lru_tree_in{vlSymsp->TOP.lru_tree_in}
    , lru_access_way{vlSymsp->TOP.lru_access_way}
    , lru_access_en{vlSymsp->TOP.lru_access_en}
    , lru_tree_out{vlSymsp->TOP.lru_tree_out}
    , lru_victim_way{vlSymsp->TOP.lru_victim_way}
    , fill_start{vlSymsp->TOP.fill_start}
    , fill_done{vlSymsp->TOP.fill_done}
    , wb_start{vlSymsp->TOP.wb_start}
    , wb_done{vlSymsp->TOP.wb_done}
    , data_rd_addr{vlSymsp->TOP.data_rd_addr}
    , data_wr_addr{vlSymsp->TOP.data_wr_addr}
    , req_vaddr{vlSymsp->TOP.req_vaddr}
    , req_data{vlSymsp->TOP.req_data}
    , resp_data{vlSymsp->TOP.resp_data}
    , tag_rd_data_0{vlSymsp->TOP.tag_rd_data_0}
    , tag_rd_data_1{vlSymsp->TOP.tag_rd_data_1}
    , tag_rd_data_2{vlSymsp->TOP.tag_rd_data_2}
    , tag_rd_data_3{vlSymsp->TOP.tag_rd_data_3}
    , tag_rd_data_4{vlSymsp->TOP.tag_rd_data_4}
    , tag_rd_data_5{vlSymsp->TOP.tag_rd_data_5}
    , tag_rd_data_6{vlSymsp->TOP.tag_rd_data_6}
    , tag_rd_data_7{vlSymsp->TOP.tag_rd_data_7}
    , tag_wr_data_0{vlSymsp->TOP.tag_wr_data_0}
    , tag_wr_data_1{vlSymsp->TOP.tag_wr_data_1}
    , tag_wr_data_2{vlSymsp->TOP.tag_wr_data_2}
    , tag_wr_data_3{vlSymsp->TOP.tag_wr_data_3}
    , tag_wr_data_4{vlSymsp->TOP.tag_wr_data_4}
    , tag_wr_data_5{vlSymsp->TOP.tag_wr_data_5}
    , tag_wr_data_6{vlSymsp->TOP.tag_wr_data_6}
    , tag_wr_data_7{vlSymsp->TOP.tag_wr_data_7}
    , data_rd_data{vlSymsp->TOP.data_rd_data}
    , data_wr_data{vlSymsp->TOP.data_wr_data}
    , fill_addr{vlSymsp->TOP.fill_addr}
    , fill_word_0{vlSymsp->TOP.fill_word_0}
    , fill_word_1{vlSymsp->TOP.fill_word_1}
    , fill_word_2{vlSymsp->TOP.fill_word_2}
    , fill_word_3{vlSymsp->TOP.fill_word_3}
    , fill_word_4{vlSymsp->TOP.fill_word_4}
    , fill_word_5{vlSymsp->TOP.fill_word_5}
    , fill_word_6{vlSymsp->TOP.fill_word_6}
    , fill_word_7{vlSymsp->TOP.fill_word_7}
    , wb_addr{vlSymsp->TOP.wb_addr}
    , wb_word_0{vlSymsp->TOP.wb_word_0}
    , wb_word_1{vlSymsp->TOP.wb_word_1}
    , wb_word_2{vlSymsp->TOP.wb_word_2}
    , wb_word_3{vlSymsp->TOP.wb_word_3}
    , wb_word_4{vlSymsp->TOP.wb_word_4}
    , wb_word_5{vlSymsp->TOP.wb_word_5}
    , wb_word_6{vlSymsp->TOP.wb_word_6}
    , wb_word_7{vlSymsp->TOP.wb_word_7}
    , rootp{&(vlSymsp->TOP)}
{
    // Register model with the context
    contextp()->addModel(this);
}

VFsmCacheCtrl::VFsmCacheCtrl(const char* _vcname__)
    : VFsmCacheCtrl(Verilated::threadContextp(), _vcname__)
{
}

//============================================================
// Destructor

VFsmCacheCtrl::~VFsmCacheCtrl() {
    delete vlSymsp;
}

//============================================================
// Evaluation function

#ifdef VL_DEBUG
void VFsmCacheCtrl___024root___eval_debug_assertions(VFsmCacheCtrl___024root* vlSelf);
#endif  // VL_DEBUG
void VFsmCacheCtrl___024root___eval_static(VFsmCacheCtrl___024root* vlSelf);
void VFsmCacheCtrl___024root___eval_initial(VFsmCacheCtrl___024root* vlSelf);
void VFsmCacheCtrl___024root___eval_settle(VFsmCacheCtrl___024root* vlSelf);
void VFsmCacheCtrl___024root___eval(VFsmCacheCtrl___024root* vlSelf);

void VFsmCacheCtrl::eval_step() {
    VL_DEBUG_IF(VL_DBG_MSGF("+++++TOP Evaluate VFsmCacheCtrl::eval_step\n"); );
#ifdef VL_DEBUG
    // Debug assertions
    VFsmCacheCtrl___024root___eval_debug_assertions(&(vlSymsp->TOP));
#endif  // VL_DEBUG
    vlSymsp->__Vm_deleter.deleteAll();
    if (VL_UNLIKELY(!vlSymsp->__Vm_didInit)) {
        vlSymsp->__Vm_didInit = true;
        VL_DEBUG_IF(VL_DBG_MSGF("+ Initial\n"););
        VFsmCacheCtrl___024root___eval_static(&(vlSymsp->TOP));
        VFsmCacheCtrl___024root___eval_initial(&(vlSymsp->TOP));
        VFsmCacheCtrl___024root___eval_settle(&(vlSymsp->TOP));
    }
    VL_DEBUG_IF(VL_DBG_MSGF("+ Eval\n"););
    VFsmCacheCtrl___024root___eval(&(vlSymsp->TOP));
    // Evaluate cleanup
    Verilated::endOfEval(vlSymsp->__Vm_evalMsgQp);
}

//============================================================
// Events and timing
bool VFsmCacheCtrl::eventsPending() { return false; }

uint64_t VFsmCacheCtrl::nextTimeSlot() {
    VL_FATAL_MT(__FILE__, __LINE__, "", "No delays in the design");
    return 0;
}

//============================================================
// Utilities

const char* VFsmCacheCtrl::name() const {
    return vlSymsp->name();
}

//============================================================
// Invoke final blocks

void VFsmCacheCtrl___024root___eval_final(VFsmCacheCtrl___024root* vlSelf);

VL_ATTR_COLD void VFsmCacheCtrl::final() {
    VFsmCacheCtrl___024root___eval_final(&(vlSymsp->TOP));
}

//============================================================
// Implementations of abstract methods from VerilatedModel

const char* VFsmCacheCtrl::hierName() const { return vlSymsp->name(); }
const char* VFsmCacheCtrl::modelName() const { return "VFsmCacheCtrl"; }
unsigned VFsmCacheCtrl::threads() const { return 1; }
void VFsmCacheCtrl::prepareClone() const { contextp()->prepareClone(); }
void VFsmCacheCtrl::atClone() const {
    contextp()->threadPoolpOnClone();
}
