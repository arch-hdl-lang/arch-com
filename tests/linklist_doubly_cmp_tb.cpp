// Comparison testbench for SchedList (linklist_doubly)
// Tests next/prev traversal, insert_after, insert_head, insert_tail, delete_head
//
// Build & run:
//   cargo run -- sim tests/linklist_doubly.arch --tb tests/linklist_doubly_cmp_tb.cpp
// Verilator:
//   verilator --cc --exe --build tests/linklist_doubly_cmp_tb.cpp tests/linklist_doubly.sv \
//     --top-module SchedList -o doubly_cmp_tb --Mdir linklist_doubly_build
//   ./linklist_doubly_build/doubly_cmp_tb

#include "VSchedList.h"
#include <cstdio>
#include <vector>

static int g_cycle = 0;

static void log(const char* op, const char* key, unsigned val) {
    printf("[cycle %3d] %-22s %s = %u\n", g_cycle, op, key, val);
}
static void log_state(VSchedList& d, const char* tag) {
    printf("[cycle %3d] STATE %-17s empty=%u full=%u length=%u\n",
           g_cycle, tag, d.empty, d.full, d.length);
}

struct DUT {
    VSchedList dut;

    void half_tick() { dut.clk ^= 1; dut.eval(); if (dut.clk) g_cycle++; }
    void tick() { half_tick(); half_tick(); }

    void reset(int cycles = 2) {
        dut.rst = 1; dut.clk = 0;
        dut.alloc_req_valid        = 0;
        dut.free_req_valid         = 0; dut.free_req_handle       = 0;
        dut.insert_head_req_valid  = 0; dut.insert_head_req_data  = 0;
        dut.insert_tail_req_valid  = 0; dut.insert_tail_req_data  = 0;
        dut.insert_after_req_valid = 0;
        dut.insert_after_req_handle = 0; dut.insert_after_req_data = 0;
        dut.delete_head_req_valid  = 0;
        dut.next_req_valid         = 0; dut.next_req_handle        = 0;
        dut.prev_req_valid         = 0; dut.prev_req_handle        = 0;
        for (int i = 0; i < cycles * 2; i++) half_tick();
        dut.rst = 0;
        g_cycle = 0;
        printf("[cycle   0] RESET\n");
        log_state(dut, "after-reset");
    }

    // latency-2 insert_head
    uint32_t insert_head_one(uint32_t data) {
        log("insert_head", "req_data", data);
        dut.insert_head_req_valid = 1; dut.insert_head_req_data = data;
        tick();
        dut.insert_head_req_valid = 0; dut.insert_head_req_data = 0;
        tick();
        uint32_t h = dut.insert_head_resp_valid ? (uint32_t)dut.insert_head_resp_handle : 0xFF;
        log("insert_head", "resp_handle", h);
        tick();
        log_state(dut, "insert_head");
        return h;
    }

    // latency-2 insert_tail
    uint32_t insert_tail_one(uint32_t data) {
        log("insert_tail", "req_data", data);
        dut.insert_tail_req_valid = 1; dut.insert_tail_req_data = data;
        tick();
        dut.insert_tail_req_valid = 0; dut.insert_tail_req_data = 0;
        tick();
        uint32_t h = dut.insert_tail_resp_valid ? (uint32_t)dut.insert_tail_resp_handle : 0xFF;
        log("insert_tail", "resp_handle", h);
        tick();
        log_state(dut, "insert_tail");
        return h;
    }

    // latency-2 insert_after
    uint32_t insert_after_one(uint32_t after_handle, uint32_t data) {
        log("insert_after", "after_handle", after_handle);
        log("insert_after", "req_data", data);
        dut.insert_after_req_valid  = 1;
        dut.insert_after_req_handle = after_handle;
        dut.insert_after_req_data   = data;
        tick();
        dut.insert_after_req_valid  = 0;
        dut.insert_after_req_handle = 0;
        dut.insert_after_req_data   = 0;
        tick();
        uint32_t h = dut.insert_after_resp_valid ? (uint32_t)dut.insert_after_resp_handle : 0xFF;
        log("insert_after", "resp_handle", h);
        tick();
        log_state(dut, "insert_after");
        return h;
    }

    // latency-2 delete_head
    uint32_t delete_head_one() {
        dut.delete_head_req_valid = 1;
        tick();
        dut.delete_head_req_valid = 0;
        tick();
        uint32_t d = dut.delete_head_resp_valid ? (uint32_t)dut.delete_head_resp_data : 0xDEAD;
        log("delete_head", "resp_data", d);
        tick();
        log_state(dut, "delete_head");
        return d;
    }

    // latency-1 next
    uint32_t next_one(uint32_t handle) {
        log("next", "req_handle", handle);
        dut.next_req_valid  = 1; dut.next_req_handle = handle;
        tick();
        uint32_t h = dut.next_resp_valid ? (uint32_t)dut.next_resp_handle : 0xFF;
        log("next", "resp_handle", h);
        dut.next_req_valid  = 0; dut.next_req_handle = 0;
        tick();
        return h;
    }

    // latency-1 prev
    uint32_t prev_one(uint32_t handle) {
        log("prev", "req_handle", handle);
        dut.prev_req_valid  = 1; dut.prev_req_handle = handle;
        tick();
        uint32_t h = dut.prev_resp_valid ? (uint32_t)dut.prev_resp_handle : 0xFF;
        log("prev", "resp_handle", h);
        dut.prev_req_valid  = 0; dut.prev_req_handle = 0;
        tick();
        return h;
    }
};

int main() {
    DUT d;

    // ── insert_tail x3 then forward/backward traversal ──────────────────────
    printf("\n--- insert_tail x3 + next/prev traversal ---\n");
    d.reset();
    uint32_t h0 = d.insert_tail_one(100);
    uint32_t h1 = d.insert_tail_one(200);
    uint32_t h2 = d.insert_tail_one(300);
    // next chain: h0 → h1 → h2
    printf("  next chain:\n");
    d.next_one(h0);    // expect h1
    d.next_one(h1);    // expect h2
    // prev chain: h2 → h1 → h0
    printf("  prev chain:\n");
    d.prev_one(h2);    // expect h1
    d.prev_one(h1);    // expect h0

    // ── insert_head x3 then traversal ───────────────────────────────────────
    printf("\n--- insert_head x3 + next/prev traversal ---\n");
    d.reset();
    uint32_t a = d.insert_head_one(10);
    uint32_t b = d.insert_head_one(20);  // list: 20 → 10
    uint32_t c = d.insert_head_one(30);  // list: 30 → 20 → 10
    // forward: c → b → a
    printf("  next chain:\n");
    d.next_one(c);     // expect b
    d.next_one(b);     // expect a
    // backward: a → b → c
    printf("  prev chain:\n");
    d.prev_one(a);     // expect b
    d.prev_one(b);     // expect c

    // ── delete_head preserves prev on new head ───────────────────────────────
    printf("\n--- delete_head then prev on new head ---\n");
    d.reset();
    uint32_t p = d.insert_tail_one(1);
    uint32_t q = d.insert_tail_one(2);
    uint32_t r = d.insert_tail_one(3);
    d.delete_head_one();   // removes p; new head = q
    // prev of q: head has no meaningful prev but next(q) should still be r
    d.next_one(q);         // expect r

    // ── insert_after ─────────────────────────────────────────────────────────
    printf("\n--- insert_after ---\n");
    d.reset();
    uint32_t x = d.insert_tail_one(1);
    uint32_t y = d.insert_tail_one(3);   // list: x(1) → y(3)
    uint32_t z = d.insert_after_one(x, 2); // insert 2 after x: x(1) → z(2) → y(3)
    printf("  next chain (expect x→z→y):\n");
    d.next_one(x);   // expect z
    d.next_one(z);   // expect y
    printf("  prev chain (expect y→z→x):\n");
    d.prev_one(y);   // expect z
    d.prev_one(z);   // expect x

    // ── mixed insert_head + insert_tail ──────────────────────────────────────
    printf("\n--- mixed head/tail inserts ---\n");
    d.reset();
    uint32_t m1 = d.insert_tail_one(5);
    uint32_t m2 = d.insert_head_one(1);  // list: 1 → 5
    uint32_t m3 = d.insert_tail_one(9);  // list: 1 → 5 → 9
    printf("  next chain:\n");
    d.next_one(m2);   // expect m1 (data=5)
    d.next_one(m1);   // expect m3 (data=9)
    printf("  prev chain:\n");
    d.prev_one(m3);   // expect m1 (data=5)
    d.prev_one(m1);   // expect m2 (data=1)

    printf("\nDone.\n");
    return 0;
}
