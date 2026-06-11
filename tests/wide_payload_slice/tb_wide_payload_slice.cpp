#include "VWidePayloadSlice.h"
#include <cstdio>
#include <cstdint>

// Drive a 66-bit payload through the slice and confirm dn_valid is a clean 1
// (not corrupted by an out-of-bounds VlWide<->_arch_u128 conversion) and that
// the wide payload survives the round-trip.
int main() {
    VWidePayloadSlice dut;
    auto tick = [&]() { dut.clk = 1; dut.eval(); dut.clk = 0; dut.eval(); };

    dut.rst = 0; dut.clk = 0; dut.up_valid = 0; dut.dn_ready = 1; dut.eval();
    for (int i = 0; i < 3; i++) tick();
    dut.rst = 1;

    // Payload occupies bits beyond word 1 (so word 2 is exercised): set a
    // distinctive value in the low 64 bits.
    dut.up_payload = VlWide<3>((uint64_t)0xCAFEF00DDEADBEEFull);
    // Set a bit in the high word (bit 64) so word[2] is non-zero too.
    dut.up_payload._data[2] = 0x1;
    dut.up_valid = 1;

    tick();
    // After one accept, the slice should present dn_valid == 1 (exactly 1,
    // not a clobbered multi-bit value).
    if (dut.dn_valid != 1) {
        printf("FAIL: dn_valid = %d (expected exactly 1) — wide-payload "
               "conversion clobbered the valid bit\n", (int)dut.dn_valid);
        return 1;
    }
    // Low 64 bits of the payload must be intact.
    uint64_t lo = (uint64_t)dut.dn_payload._data[0]
                | ((uint64_t)dut.dn_payload._data[1] << 32);
    if (lo != 0xCAFEF00DDEADBEEFull) {
        printf("FAIL: dn_payload low64 = 0x%llx (expected 0xCAFEF00DDEADBEEF)\n",
               (unsigned long long)lo);
        return 1;
    }
    if ((dut.dn_payload._data[2] & 0x3) != 0x1) {
        printf("FAIL: dn_payload bit64 lost: word2=0x%x\n",
               (unsigned)dut.dn_payload._data[2]);
        return 1;
    }
    printf("PASS wide_payload_slice: dn_valid=1, 66-bit payload intact\n");
    return 0;
}
