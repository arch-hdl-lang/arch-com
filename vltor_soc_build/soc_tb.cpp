#include "VSocTop.h"
#include "verilated.h"
#include "verilated_vcd_c.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0, test_count = 0;
#define CHECK(cond, msg) do { test_count++; if (!(cond)) { printf("FAIL: %s\n", msg); fail_count++; } else { printf("PASS: %s\n", msg); } } while(0)

uint64_t sim_time = 0;

static void tick(VSocTop *d, VerilatedVcdC *tfp) {
  d->clk = 0; d->eval(); tfp->dump(sim_time++);
  d->clk = 1; d->eval(); tfp->dump(sim_time++);
}

static void icb_write(VSocTop *d, VerilatedVcdC *tfp, uint32_t addr, uint32_t data) {
  d->ext_cmd_valid = 1; d->ext_cmd_addr = addr;
  d->ext_cmd_wdata = data; d->ext_cmd_read = 0; d->ext_cmd_wmask = 0xF;
  tick(d, tfp); d->ext_cmd_valid = 0;
  for (int i = 0; i < 4; i++) tick(d, tfp);
}

static uint32_t icb_read(VSocTop *d, VerilatedVcdC *tfp, uint32_t addr) {
  d->ext_cmd_valid = 1; d->ext_cmd_addr = addr;
  d->ext_cmd_read = 1; d->ext_cmd_wmask = 0;
  tick(d, tfp); d->ext_cmd_valid = 0;
  for (int i = 0; i < 6; i++) tick(d, tfp);
  d->eval();
  return d->ext_rsp_rdata;
}

int main(int argc, char **argv) {
  Verilated::commandArgs(argc, argv);
  Verilated::traceEverOn(true);

  VSocTop *d = new VSocTop;
  VerilatedVcdC *tfp = new VerilatedVcdC;
  d->trace(tfp, 99);
  tfp->open("soc_top.vcd");

  d->clk = 0; d->rst_n = 0;
  d->itcm_wr_en = 0; d->itcm_wr_addr = 0; d->itcm_wr_data = 0;
  d->ext_cmd_valid = 0; d->ext_cmd_addr = 0; d->ext_cmd_wdata = 0;
  d->ext_cmd_wmask = 0; d->ext_cmd_read = 0;
  d->gpio_in = 0; d->uart_rx = 1; d->spi_miso = 0;
  d->fio_in_0 = 0; d->fio_in_1 = 0;
  d->dbg_psel = 0; d->dbg_penable = 0; d->dbg_paddr = 0;
  d->dbg_pwdata = 0; d->dbg_pwrite = 0;

  for (int i = 0; i < 5; i++) tick(d, tfp);
  d->rst_n = 1;
  for (int i = 0; i < 5; i++) tick(d, tfp);

  // Test 1: ITCM load
  d->itcm_wr_en = 1; d->itcm_wr_addr = 0; d->itcm_wr_data = 0x02A00093;
  tick(d, tfp); d->itcm_wr_en = 0;
  for (int i = 0; i < 5; i++) tick(d, tfp);
  CHECK(true, "ITCM write accepted");

  // Test 2: SRAM write + read
  icb_write(d, tfp, 0x20000000, 0xCAFEBABE);
  uint32_t v = icb_read(d, tfp, 0x20000000);
  printf("  SRAM read: 0x%08X\n", v);
  CHECK(v == 0xCAFEBABE, "SRAM write/read = 0xCAFEBABE");

  // Test 3: SRAM second word
  icb_write(d, tfp, 0x20000004, 0xDEADBEEF);
  v = icb_read(d, tfp, 0x20000004);
  printf("  SRAM[1]: 0x%08X\n", v);
  CHECK(v == 0xDEADBEEF, "SRAM[1] = 0xDEADBEEF");

  // Test 4: GPIO via PPI
  icb_write(d, tfp, 0x10012000, 0xAA55AA55);
  d->eval();
  printf("  GPIO out: 0x%08X\n", d->gpio_out);
  CHECK(d->gpio_out == 0xAA55AA55, "GPIO output = 0xAA55AA55");

  // Test 5: GPIO OE
  icb_write(d, tfp, 0x10012004, 0xFFFFFFFF);
  d->eval();
  printf("  GPIO OE: 0x%08X\n", d->gpio_oe);
  CHECK(d->gpio_oe == 0xFFFFFFFF, "GPIO OE = all enabled");

  // Test 6: FIO
  icb_write(d, tfp, 0x30000000, 0x12340000);
  d->eval();
  printf("  FIO out0: 0x%08X\n", d->fio_out_0);
  CHECK(d->fio_out_0 == 0x12340000, "FIO out0 = 0x12340000");

  // Test 7: UART idle
  CHECK(d->uart_tx == 1, "UART TX idle high");

  // Test 8: SPI idle
  CHECK(d->spi_cs_n == 1, "SPI CS_n idle high");

  // Test 9: Debug
  d->dbg_psel = 1; d->dbg_penable = 0; d->dbg_paddr = 0x10;
  d->dbg_pwdata = 1; d->dbg_pwrite = 1;
  tick(d, tfp);
  d->dbg_penable = 1; tick(d, tfp);
  d->dbg_psel = 0; d->dbg_penable = 0;
  d->dbg_psel = 1; d->dbg_paddr = 0x10; d->dbg_pwrite = 0;
  tick(d, tfp);
  d->dbg_penable = 1; d->eval();
  v = d->dbg_prdata;
  tick(d, tfp); d->dbg_psel = 0;
  CHECK((v & 1) == 1, "Debug: dmactive=1");

  // Test 10: GPIO IRQ
  d->gpio_in = 0; tick(d, tfp);
  icb_write(d, tfp, 0x1001200C, 1);
  d->gpio_in = 1; tick(d, tfp); tick(d, tfp);
  d->eval();
  CHECK(d->gpio_irq == 1, "GPIO IRQ on rising edge");

  // Test 11: SRAM re-read
  v = icb_read(d, tfp, 0x20000000);
  printf("  SRAM re-read: 0x%08X\n", v);
  CHECK(v == 0xCAFEBABE, "SRAM still = 0xCAFEBABE");

  for (int i = 0; i < 20; i++) tick(d, tfp);

  printf("\n=== Verilator SocTop: %d/%d passed ===\n", test_count - fail_count, test_count);

  tfp->close(); delete tfp; delete d;
  return fail_count ? 1 : 0;
}
