#include "VL1DCache.h"
#include "verilated.h"
#include <cstdio>
#include <cstdlib>
#include <cstdint>
#include <cstring>
#include <map>

static VL1DCache* dut;

static std::map<uint64_t, uint64_t> mem_model;

static uint64_t mem_read(uint64_t byte_addr) {
    uint64_t wa = byte_addr >> 3;
    auto it = mem_model.find(wa);
    if (it != mem_model.end()) return it->second;
    return 0xFEED000000000000ULL | (byte_addr & ~7ULL);
}

static void mem_write(uint64_t byte_addr, uint64_t data, uint8_t strb = 0xFF) {
    uint64_t wa  = byte_addr >> 3;
    uint64_t old = mem_read(byte_addr);
    uint64_t res = old;
    for (int b = 0; b < 8; b++)
        if ((strb >> b) & 1)
            res = (res & ~(0xFFULL << (b*8))) | (((data >> (b*8)) & 0xFF) << (b*8));
    mem_model[wa] = res;
}

static bool    r_active  = false;
static uint64_t r_base   = 0;
static int      r_beat   = 0;

static void axi_drive() {
    dut->ar_ready = r_active ? 0 : 1;
    if (r_active) {
        uint64_t waddr = r_base + (uint64_t)r_beat * 8;
        dut->r_valid = 1;
        dut->r_data  = mem_read(waddr);
        dut->r_id    = 0;
        dut->r_resp  = 0;
        dut->r_last  = (r_beat == 7) ? 1 : 0;
    } else {
        dut->r_valid = 0; dut->r_data = 0;
        dut->r_id = 0; dut->r_resp = 0; dut->r_last = 0;
    }
    dut->aw_ready = 1;
    dut->w_ready = 0;
    dut->b_valid = 0; dut->b_id = 0; dut->b_resp = 0;
}

static int cyc = 0;

static void tick(int n = 1) {
    for (int i = 0; i < n; i++) {
        axi_drive();
        dut->eval();

        bool ar_hs     = dut->ar_valid && dut->ar_ready;
        uint64_t ar_a  = (uint64_t)dut->ar_addr;
        bool r_hs      = r_active && dut->r_valid && dut->r_ready;

        printf("PRE  cyc=%d: req_ready=%d resp_valid=%d ar_valid=%d r_ready=%d r_active=%d r_beat=%d\n",
               cyc, dut->req_ready, dut->resp_valid, dut->ar_valid, dut->r_ready, r_active, r_beat);

        dut->clk = 0; dut->eval();
        dut->clk = 1; dut->eval();
        cyc++;

        if (ar_hs && !r_active) {
            r_base = ar_a & ~63ULL; r_beat = 0; r_active = true;
        }
        if (r_hs) {
            if (r_beat == 7) r_active = false;
            else             r_beat++;
        }

        axi_drive();
        dut->eval();

        printf("POST cyc=%d: req_ready=%d resp_valid=%d ar_valid=%d r_ready=%d r_active=%d r_beat=%d\n",
               cyc, dut->req_ready, dut->resp_valid, dut->ar_valid, dut->r_ready, r_active, r_beat);
    }
}

int main(int argc, char** argv) {
    VerilatedContext* ctx = new VerilatedContext;
    dut = new VL1DCache(ctx);

    dut->rst = 1; dut->clk = 0;
    dut->req_valid = 0; dut->req_vaddr = 0; dut->req_data = 0;
    dut->req_be = 0; dut->req_is_store = 0;
    dut->ar_ready = 0; dut->r_valid = 0; dut->r_data = 0;
    dut->r_id = 0; dut->r_resp = 0; dut->r_last = 0;
    dut->aw_ready = 0; dut->w_ready = 0; dut->w_data = 0; dut->w_strb = 0; dut->w_last = 0;
    dut->b_valid = 0; dut->b_id = 0; dut->b_resp = 0;
    dut->eval();
    tick(3);
    dut->rst = 0; tick(2);

    uint64_t addr1 = 0x000000000018ULL;
    uint64_t line1 = addr1 & ~63ULL;
    for (int b = 0; b < 8; b++)
        mem_write(line1 + b*8, 0xA000000000000000ULL | (line1 + b*8));

    printf("=== Starting load from 0x%llx ===\n", (unsigned long long)addr1);
    dut->req_valid = 1; dut->req_vaddr = addr1;
    dut->req_data = 0; dut->req_be = 0xFF; dut->req_is_store = 0;

    int t = 30;
    while (!dut->req_ready && --t) tick(1);
    printf("Got req_ready after %d waits\n", 30-t);
    tick(1); dut->req_valid = 0;

    t = 50;
    while (!dut->resp_valid && --t) tick(1);
    if (!t) {
        printf("TIMEOUT waiting for resp_valid\n");
        delete dut; delete ctx;
        return 1;
    }
    printf("PASS: resp_data=0x%llx\n", (unsigned long long)dut->resp_data);
    delete dut; delete ctx;
    return 0;
}
