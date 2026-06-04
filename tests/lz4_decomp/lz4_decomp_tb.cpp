// LZ4 block decompressor testbench
//
// Tests three LZ4 block-format vectors:
//   1. Pure literals — no back-references
//   2. Literals + single back-reference (overlapping copy)
//   3. Run-length via match offset=1 (repeat-last-byte copy)
//
// Sampling protocol: combinational outputs are read at the negedge (clk=0
// eval) of each cycle so the FSM's registered state transitions do not yet
// affect the inputs the comb logic sees.  The input index advances only
// after a confirmed in_ready=1 sample.

#include "VLz4Decomp.h"
#include "verilated.h"
#include <cstdio>
#include <cstdint>
#include <vector>
#include <cstring>

static int g_pass = 0, g_fail = 0;

#define CHECK(cond, ...) do {                          \
  if (cond) { printf("  PASS: " __VA_ARGS__); printf("\n"); ++g_pass; } \
  else      { printf("  FAIL: " __VA_ARGS__); printf("\n"); ++g_fail; } \
} while (0)

static VLz4Decomp* dut;

static void reset() {
    dut->clk = 0;
    dut->rst = 1;
    dut->in_valid = 0;
    dut->in_data  = 0;
    dut->in_last  = 0;
    dut->out_ready = 1;
    dut->eval();
    // Two reset cycles
    for (int i = 0; i < 2; i++) {
        dut->clk = 1; dut->eval();
        dut->clk = 0; dut->eval();
    }
    dut->rst = 0;
    dut->clk = 1; dut->eval();
    dut->clk = 0; dut->eval();
}

// Run the decompressor on 'compressed' (length n), collect output bytes.
// Returns the collected output vector.
// Max cycles: 4096 (safety stop).
static std::vector<uint8_t> decompress(
        const uint8_t* compressed, int n,
        int max_cycles = 4096)
{
    std::vector<uint8_t> out;
    int in_idx = 0;
    bool consumed_prev = false;

    dut->out_ready = 1;

    for (int cyc = 0; cyc < max_cycles; cyc++) {
        // Advance input index if previous cycle consumed the byte
        if (consumed_prev && in_idx < n)
            in_idx++;

        // Drive inputs for this cycle
        if (in_idx < n) {
            dut->in_valid = 1;
            dut->in_data  = compressed[in_idx];
            dut->in_last  = (in_idx == n - 1) ? 1 : 0;
        } else {
            dut->in_valid = 0;
            dut->in_data  = 0;
            dut->in_last  = 0;
        }

        // Negedge eval: combinational outputs reflect current state + inputs
        dut->clk = 0;
        dut->eval();

        // Sample combinational outputs BEFORE the posedge state transition
        consumed_prev = (bool)dut->in_ready;
        if (dut->out_valid)
            out.push_back((uint8_t)dut->out_data);

        // Posedge: state registers update
        dut->clk = 1;
        dut->eval();

        // Termination: all input consumed AND DUT produced nothing this cycle
        if (in_idx >= n - 1 && consumed_prev && !dut->out_valid) {
            // A few more cycles to drain any pending MatchCopy output
            bool any = false;
            for (int drain = 0; drain < 256; drain++) {
                dut->in_valid = 0;
                dut->clk = 0; dut->eval();
                if (dut->out_valid) { out.push_back((uint8_t)dut->out_data); any = true; }
                dut->clk = 1; dut->eval();
                if (!dut->out_valid && !any) break;
                any = false;
            }
            break;
        }
    }
    return out;
}

static void run_test(const char* name,
                     const uint8_t* compressed, int clen,
                     const uint8_t* expected,   int elen)
{
    printf("=== %s ===\n", name);
    reset();
    std::vector<uint8_t> got = decompress(compressed, clen);

    printf("  compressed  : %d bytes\n", clen);
    printf("  expected out: %d bytes\n", elen);
    printf("  got out     : %d bytes\n", (int)got.size());

    CHECK((int)got.size() == elen, "output length %d == expected %d", (int)got.size(), elen);
    if ((int)got.size() == elen) {
        bool ok = (memcmp(got.data(), expected, elen) == 0);
        CHECK(ok, "output bytes match");
        if (!ok) {
            printf("  Expected: ");
            for (int i = 0; i < elen; i++) printf("%02x ", expected[i]);
            printf("\n  Got:      ");
            for (int i = 0; i < (int)got.size(); i++) printf("%02x ", got[i]);
            printf("\n");
        }
    }
}

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    dut = new VLz4Decomp;

    // ─── Test 1: pure literals "HELLO" ────────────────────────────────────
    // Token 0x50: high nibble=5 (5 literals), low nibble=0
    // Literals: H E L L O (0x48 45 4C 4C 4F)
    // Last byte is the final literal — in_last asserted there.
    {
        static const uint8_t comp[] = { 0x50, 0x48, 0x45, 0x4C, 0x4C, 0x4F };
        static const uint8_t expe[] = { 0x48, 0x45, 0x4C, 0x4C, 0x4F };
        run_test("Test 1: pure literals \"HELLO\"",
                 comp, (int)sizeof(comp),
                 expe, (int)sizeof(expe));
    }

    // ─── Test 2: back-reference "ABCABCABC" ───────────────────────────────
    // Sequence 1: token 0x32 → 3 literals ABC + match(offset=3, len=6)
    //   token high=3 (3 literals), low=2 (match_len = 2+4 = 6)
    //   literals: A B C
    //   offset LE16: [0x03, 0x00]
    // Sequence 2 (last): token 0x00 → 0 literals, in_last on this byte
    // Output: ABCABCABC (9 bytes)
    {
        static const uint8_t comp[] = { 0x32, 0x41, 0x42, 0x43, 0x03, 0x00, 0x00 };
        static const uint8_t expe[] = { 0x41, 0x42, 0x43, 0x41, 0x42, 0x43, 0x41, 0x42, 0x43 };
        run_test("Test 2: back-reference \"ABCABCABC\"",
                 comp, (int)sizeof(comp),
                 expe, (int)sizeof(expe));
    }

    // ─── Test 3: run-length "AAAAAAAAAA" (10 × 'A') ───────────────────────
    // Sequence 1: token 0x42 → 4 literals AAAA + match(offset=1, len=6)
    //   token high=4 (4 literals), low=2 (match_len = 2+4 = 6)
    //   literals: A A A A
    //   offset LE16: [0x01, 0x00]  (repeat the byte just written)
    // Sequence 2 (last): token 0x00, in_last
    // Overlapping copy from offset 1 extends the run: AAAA + AAAAAA = AAAAAAAAAA
    {
        static const uint8_t comp[] = { 0x42, 0x41, 0x41, 0x41, 0x41, 0x01, 0x00, 0x00 };
        static const uint8_t expe[] = { 0x41,0x41,0x41,0x41, 0x41,0x41,0x41,0x41,0x41,0x41 };
        run_test("Test 3: run-length \"AAAAAAAAAA\"",
                 comp, (int)sizeof(comp),
                 expe, (int)sizeof(expe));
    }

    // ─── Test 4: extended literal length (nibble == 15) ───────────────────
    // Build 16 literals 0x00..0x0F (to trigger lit-len extension).
    // Token high=15 → need extension; extension byte = 1 → total lit_len = 16
    // Token 0xF0: high=15, low=0
    // Extension byte: 0x01 (lit_len = 15 + 1 = 16)
    // Literals: 0x00 0x01 ... 0x0F
    // in_last on last literal byte (0x0F)
    {
        static const uint8_t comp[] = {
            0xF0,                    // token: lit_len base=15, match_extra=0
            0x01,                    // extension: +1 → total lit_len=16
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
            0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F
        };
        static const uint8_t expe[] = {
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
            0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F
        };
        run_test("Test 4: extended lit-len (16 bytes)",
                 comp, (int)sizeof(comp),
                 expe, (int)sizeof(expe));
    }

    dut->final();
    delete dut;

    printf("\n=== Summary: %d passed, %d failed ===\n", g_pass, g_fail);
    return (g_fail == 0) ? 0 : 1;
}
