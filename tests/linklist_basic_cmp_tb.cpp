// Comparison testbench for TaskQueue
// Produces identical log output when compiled against either:
//   - arch sim model  (arch_sim_build/VTaskQueue.h)
//   - Verilator model (linklist_basic_build/VTaskQueue.h)
//
// Run via: make -C tests linklist_cmp
// Or manually (see bottom of file).

#include "VTaskQueue.h"
#include <cstdio>
#include <vector>

// ── Logging ──────────────────────────────────────────────────────────────────

static int g_cycle = 0;

static void log(const char* op, const char* key, unsigned val) {
    printf("[cycle %3d] %-20s %s = %u\n", g_cycle, op, key, val);
}

static void log_state(VTaskQueue& d, const char* tag) {
    printf("[cycle %3d] STATE %-15s empty=%u full=%u length=%u\n",
           g_cycle, tag, d.empty, d.full, d.length);
}

// ── DUT wrapper ──────────────────────────────────────────────────────────────

struct DUT {
    VTaskQueue dut;

    void half_tick() {
        dut.clk ^= 1;
        dut.eval();
        if (dut.clk) g_cycle++; // count rising edges
    }

    void tick() { half_tick(); half_tick(); }

    void reset(int cycles = 2) {
        dut.rst = 1; dut.clk = 0;
        dut.alloc_req_valid       = 0;
        dut.free_req_valid        = 0; dut.free_req_handle      = 0;
        dut.insert_tail_req_valid = 0; dut.insert_tail_req_data = 0;
        dut.delete_head_req_valid = 0;
        dut.read_data_req_valid   = 0; dut.read_data_req_handle = 0;
        dut.write_data_req_valid  = 0;
        dut.write_data_req_handle = 0; dut.write_data_req_data  = 0;
        for (int i = 0; i < cycles * 2; i++) half_tick();
        dut.rst = 0;
        g_cycle = 0;
        printf("[cycle   0] RESET\n");
        log_state(dut, "after-reset");
    }

    uint32_t alloc_one() {
        dut.alloc_req_valid = 1;
        tick();
        uint32_t h = dut.alloc_resp_valid ? (uint32_t)dut.alloc_resp_handle : 0xFF;
        log("alloc", "handle", h);
        dut.alloc_req_valid = 0;
        tick();
        log_state(dut, "alloc");
        return h;
    }

    void free_one(uint32_t handle) {
        log("free", "handle", handle);
        dut.free_req_valid = 1; dut.free_req_handle = handle;
        tick();
        dut.free_req_valid = 0; dut.free_req_handle = 0;
        tick();
        log_state(dut, "free");
    }

    uint32_t insert_tail_one(uint32_t data) {
        log("insert_tail", "req_data", data);
        dut.insert_tail_req_valid = 1; dut.insert_tail_req_data = data;
        tick();
        dut.insert_tail_req_valid = 0; dut.insert_tail_req_data = 0;
        tick();
        uint32_t h = dut.insert_tail_resp_valid
                   ? (uint32_t)dut.insert_tail_resp_handle : 0xFF;
        log("insert_tail", "resp_handle", h);
        tick();
        log_state(dut, "insert_tail");
        return h;
    }

    uint32_t delete_head_one() {
        dut.delete_head_req_valid = 1;
        tick();
        dut.delete_head_req_valid = 0;
        tick();
        uint32_t d = dut.delete_head_resp_valid
                   ? (uint32_t)dut.delete_head_resp_data : 0xDEAD;
        log("delete_head", "resp_data", d);
        tick();
        log_state(dut, "delete_head");
        return d;
    }

    uint32_t read_data_one(uint32_t handle) {
        log("read_data", "req_handle", handle);
        dut.read_data_req_valid = 1; dut.read_data_req_handle = handle;
        tick();
        uint32_t d = dut.read_data_resp_valid
                   ? (uint32_t)dut.read_data_resp_data : 0xDEAD;
        log("read_data", "resp_data", d);
        dut.read_data_req_valid = 0; dut.read_data_req_handle = 0;
        tick();
        return d;
    }

    void write_data_one(uint32_t handle, uint32_t data) {
        log("write_data", "req_handle", handle);
        log("write_data", "req_data",   data);
        dut.write_data_req_valid  = 1;
        dut.write_data_req_handle = handle;
        dut.write_data_req_data   = data;
        tick();
        dut.write_data_req_valid = 0;
        tick();
    }
};

// ── Test sequence (canonical — same output expected from both simulators) ────

int main() {
    DUT d;

    // ── Reset ──
    d.reset();

    // ── Single insert + delete ──
    printf("\n--- single insert/delete ---\n");
    d.insert_tail_one(0xABCD1234);
    d.delete_head_one();

    // ── FIFO order: 4 elements ──
    printf("\n--- fifo order ---\n");
    d.insert_tail_one(10);
    d.insert_tail_one(20);
    d.insert_tail_one(30);
    d.insert_tail_one(40);
    d.delete_head_one();
    d.delete_head_one();
    d.delete_head_one();
    d.delete_head_one();

    // ── Fill to capacity (DEPTH=8) ──
    printf("\n--- fill to capacity ---\n");
    for (int i = 0; i < 8; i++) d.insert_tail_one(0x100 + i);
    log_state(d.dut, "full");
    for (int i = 0; i < 8; i++) d.delete_head_one();
    log_state(d.dut, "drained");

    // ── read_data / write_data ──
    printf("\n--- read/write data ---\n");
    uint32_t h = d.insert_tail_one(0xDEADBEEF);
    d.read_data_one(h);
    d.write_data_one(h, 0xCAFEBABE);
    d.read_data_one(h);
    d.delete_head_one();

    // ── alloc / free (no data insert) ──
    printf("\n--- alloc/free ---\n");
    std::vector<uint32_t> handles;
    for (int i = 0; i < 4; i++) handles.push_back(d.alloc_one());
    for (auto hh : handles) d.free_one(hh);

    // ── Interleaved insert/delete ──
    printf("\n--- interleaved ---\n");
    d.insert_tail_one(1); d.insert_tail_one(2); d.insert_tail_one(3);
    d.delete_head_one();
    d.insert_tail_one(4); d.insert_tail_one(5);
    d.delete_head_one(); d.delete_head_one();
    d.delete_head_one(); d.delete_head_one();

    printf("\nDone.\n");
    return 0;
}
