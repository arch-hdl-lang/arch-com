// E203 HBirdv2 Bus Interface Unit (BIU)
// Arbitrates LSU and IFU ICB requests, routes to downstream targets
// (PPI, CLINT, PLIC, FIO, MEM) based on address region indicators.
// Matches RealBench port interface.
module e203_biu (
  input logic clk,
  input logic rst_n,
  output logic biu_active,
  input logic lsu2biu_icb_cmd_valid,
  output logic lsu2biu_icb_cmd_ready,
  input logic [32-1:0] lsu2biu_icb_cmd_addr,
  input logic lsu2biu_icb_cmd_read,
  input logic [32-1:0] lsu2biu_icb_cmd_wdata,
  input logic [4-1:0] lsu2biu_icb_cmd_wmask,
  input logic [2-1:0] lsu2biu_icb_cmd_burst,
  input logic [2-1:0] lsu2biu_icb_cmd_beat,
  input logic lsu2biu_icb_cmd_lock,
  input logic lsu2biu_icb_cmd_excl,
  input logic [2-1:0] lsu2biu_icb_cmd_size,
  output logic lsu2biu_icb_rsp_valid,
  input logic lsu2biu_icb_rsp_ready,
  output logic lsu2biu_icb_rsp_err,
  output logic lsu2biu_icb_rsp_excl_ok,
  output logic [32-1:0] lsu2biu_icb_rsp_rdata,
  input logic ifu2biu_icb_cmd_valid,
  output logic ifu2biu_icb_cmd_ready,
  input logic [32-1:0] ifu2biu_icb_cmd_addr,
  input logic ifu2biu_icb_cmd_read,
  input logic [32-1:0] ifu2biu_icb_cmd_wdata,
  input logic [4-1:0] ifu2biu_icb_cmd_wmask,
  input logic [2-1:0] ifu2biu_icb_cmd_burst,
  input logic [2-1:0] ifu2biu_icb_cmd_beat,
  input logic ifu2biu_icb_cmd_lock,
  input logic ifu2biu_icb_cmd_excl,
  input logic [2-1:0] ifu2biu_icb_cmd_size,
  output logic ifu2biu_icb_rsp_valid,
  input logic ifu2biu_icb_rsp_ready,
  output logic ifu2biu_icb_rsp_err,
  output logic ifu2biu_icb_rsp_excl_ok,
  output logic [32-1:0] ifu2biu_icb_rsp_rdata,
  input logic [32-1:0] ppi_region_indic,
  input logic ppi_icb_enable,
  output logic ppi_icb_cmd_valid,
  input logic ppi_icb_cmd_ready,
  output logic [32-1:0] ppi_icb_cmd_addr,
  output logic ppi_icb_cmd_read,
  output logic [32-1:0] ppi_icb_cmd_wdata,
  output logic [4-1:0] ppi_icb_cmd_wmask,
  output logic [2-1:0] ppi_icb_cmd_burst,
  output logic [2-1:0] ppi_icb_cmd_beat,
  output logic ppi_icb_cmd_lock,
  output logic ppi_icb_cmd_excl,
  output logic [2-1:0] ppi_icb_cmd_size,
  input logic ppi_icb_rsp_valid,
  output logic ppi_icb_rsp_ready,
  input logic ppi_icb_rsp_err,
  input logic ppi_icb_rsp_excl_ok,
  input logic [32-1:0] ppi_icb_rsp_rdata,
  input logic [32-1:0] clint_region_indic,
  input logic clint_icb_enable,
  output logic clint_icb_cmd_valid,
  input logic clint_icb_cmd_ready,
  output logic [32-1:0] clint_icb_cmd_addr,
  output logic clint_icb_cmd_read,
  output logic [32-1:0] clint_icb_cmd_wdata,
  output logic [4-1:0] clint_icb_cmd_wmask,
  output logic [2-1:0] clint_icb_cmd_burst,
  output logic [2-1:0] clint_icb_cmd_beat,
  output logic clint_icb_cmd_lock,
  output logic clint_icb_cmd_excl,
  output logic [2-1:0] clint_icb_cmd_size,
  input logic clint_icb_rsp_valid,
  output logic clint_icb_rsp_ready,
  input logic clint_icb_rsp_err,
  input logic clint_icb_rsp_excl_ok,
  input logic [32-1:0] clint_icb_rsp_rdata,
  input logic [32-1:0] plic_region_indic,
  input logic plic_icb_enable,
  output logic plic_icb_cmd_valid,
  input logic plic_icb_cmd_ready,
  output logic [32-1:0] plic_icb_cmd_addr,
  output logic plic_icb_cmd_read,
  output logic [32-1:0] plic_icb_cmd_wdata,
  output logic [4-1:0] plic_icb_cmd_wmask,
  output logic [2-1:0] plic_icb_cmd_burst,
  output logic [2-1:0] plic_icb_cmd_beat,
  output logic plic_icb_cmd_lock,
  output logic plic_icb_cmd_excl,
  output logic [2-1:0] plic_icb_cmd_size,
  input logic plic_icb_rsp_valid,
  output logic plic_icb_rsp_ready,
  input logic plic_icb_rsp_err,
  input logic plic_icb_rsp_excl_ok,
  input logic [32-1:0] plic_icb_rsp_rdata,
  input logic [32-1:0] fio_region_indic,
  input logic fio_icb_enable,
  output logic fio_icb_cmd_valid,
  input logic fio_icb_cmd_ready,
  output logic [32-1:0] fio_icb_cmd_addr,
  output logic fio_icb_cmd_read,
  output logic [32-1:0] fio_icb_cmd_wdata,
  output logic [4-1:0] fio_icb_cmd_wmask,
  output logic [2-1:0] fio_icb_cmd_burst,
  output logic [2-1:0] fio_icb_cmd_beat,
  output logic fio_icb_cmd_lock,
  output logic fio_icb_cmd_excl,
  output logic [2-1:0] fio_icb_cmd_size,
  input logic fio_icb_rsp_valid,
  output logic fio_icb_rsp_ready,
  input logic fio_icb_rsp_err,
  input logic fio_icb_rsp_excl_ok,
  input logic [32-1:0] fio_icb_rsp_rdata,
  input logic mem_icb_enable,
  output logic mem_icb_cmd_valid,
  input logic mem_icb_cmd_ready,
  output logic [32-1:0] mem_icb_cmd_addr,
  output logic mem_icb_cmd_read,
  output logic [32-1:0] mem_icb_cmd_wdata,
  output logic [4-1:0] mem_icb_cmd_wmask,
  output logic [2-1:0] mem_icb_cmd_burst,
  output logic [2-1:0] mem_icb_cmd_beat,
  output logic mem_icb_cmd_lock,
  output logic mem_icb_cmd_excl,
  output logic [2-1:0] mem_icb_cmd_size,
  input logic mem_icb_rsp_valid,
  output logic mem_icb_rsp_ready,
  input logic mem_icb_rsp_err,
  input logic mem_icb_rsp_excl_ok,
  input logic [32-1:0] mem_icb_rsp_rdata
);

  // ── LSU to BIU ICB interface ──────────────────────────────────────
  // ── IFU to BIU ICB interface ──────────────────────────────────────
  // ── PPI downstream ───────────────────────────────────────────────
  // ── CLINT downstream ─────────────────────────────────────────────
  // ── PLIC downstream ──────────────────────────────────────────────
  // ── FIO downstream ───────────────────────────────────────────────
  // ── MEM downstream (default) ─────────────────────────────────────
  // ── Arbiter: LSU has priority over IFU ────────────────────────────
  logic lsu_win;
  assign lsu_win = lsu2biu_icb_cmd_valid;
  logic arb_valid;
  assign arb_valid = lsu2biu_icb_cmd_valid | ifu2biu_icb_cmd_valid;
  logic [32-1:0] arb_addr;
  assign arb_addr = lsu_win ? lsu2biu_icb_cmd_addr : ifu2biu_icb_cmd_addr;
  logic arb_read;
  assign arb_read = lsu_win ? lsu2biu_icb_cmd_read : ifu2biu_icb_cmd_read;
  logic [32-1:0] arb_wdata;
  assign arb_wdata = lsu_win ? lsu2biu_icb_cmd_wdata : ifu2biu_icb_cmd_wdata;
  logic [4-1:0] arb_wmask;
  assign arb_wmask = lsu_win ? lsu2biu_icb_cmd_wmask : ifu2biu_icb_cmd_wmask;
  logic [2-1:0] arb_burst;
  assign arb_burst = lsu_win ? lsu2biu_icb_cmd_burst : ifu2biu_icb_cmd_burst;
  logic [2-1:0] arb_beat;
  assign arb_beat = lsu_win ? lsu2biu_icb_cmd_beat : ifu2biu_icb_cmd_beat;
  logic arb_lock;
  assign arb_lock = lsu_win ? lsu2biu_icb_cmd_lock : ifu2biu_icb_cmd_lock;
  logic arb_excl;
  assign arb_excl = lsu_win ? lsu2biu_icb_cmd_excl : ifu2biu_icb_cmd_excl;
  logic [2-1:0] arb_size;
  assign arb_size = lsu_win ? lsu2biu_icb_cmd_size : ifu2biu_icb_cmd_size;
  // ── Address decode ────────────────────────────────────────��───────
  logic is_ppi;
  assign is_ppi = ppi_icb_enable & arb_addr[31:16] == ppi_region_indic[31:16];
  logic is_clint;
  assign is_clint = clint_icb_enable & arb_addr[31:16] == clint_region_indic[31:16];
  logic is_plic;
  assign is_plic = plic_icb_enable & arb_addr[31:16] == plic_region_indic[31:16];
  logic is_fio;
  assign is_fio = fio_icb_enable & arb_addr[31:16] == fio_region_indic[31:16];
  logic is_mem;
  assign is_mem = mem_icb_enable & ~is_ppi & ~is_clint & ~is_plic & ~is_fio;
  // Track which downstream port is selected for response routing
  logic sel_ppi_r = 0;
  logic sel_clint_r = 0;
  logic sel_plic_r = 0;
  logic sel_fio_r = 0;
  logic sel_mem_r = 0;
  logic sel_lsu_r = 0;
  // Downstream ready mux
  logic arb_cmd_ready;
  assign arb_cmd_ready = is_ppi & ppi_icb_cmd_ready | is_clint & clint_icb_cmd_ready | is_plic & plic_icb_cmd_ready | is_fio & fio_icb_cmd_ready | is_mem & mem_icb_cmd_ready;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      sel_clint_r <= 0;
      sel_fio_r <= 0;
      sel_lsu_r <= 0;
      sel_mem_r <= 0;
      sel_plic_r <= 0;
      sel_ppi_r <= 0;
    end else begin
      if (arb_valid & arb_cmd_ready) begin
        sel_ppi_r <= is_ppi;
        sel_clint_r <= is_clint;
        sel_plic_r <= is_plic;
        sel_fio_r <= is_fio;
        sel_mem_r <= is_mem;
        sel_lsu_r <= lsu_win;
      end
    end
  end
  // Response mux
  logic rsp_valid;
  assign rsp_valid = sel_ppi_r & ppi_icb_rsp_valid | sel_clint_r & clint_icb_rsp_valid | sel_plic_r & plic_icb_rsp_valid | sel_fio_r & fio_icb_rsp_valid | sel_mem_r & mem_icb_rsp_valid;
  logic rsp_err;
  assign rsp_err = sel_ppi_r & ppi_icb_rsp_err | sel_clint_r & clint_icb_rsp_err | sel_plic_r & plic_icb_rsp_err | sel_fio_r & fio_icb_rsp_err | sel_mem_r & mem_icb_rsp_err;
  logic rsp_excl_ok;
  assign rsp_excl_ok = sel_ppi_r & ppi_icb_rsp_excl_ok | sel_clint_r & clint_icb_rsp_excl_ok | sel_plic_r & plic_icb_rsp_excl_ok | sel_fio_r & fio_icb_rsp_excl_ok | sel_mem_r & mem_icb_rsp_excl_ok;
  logic [32-1:0] rsp_rdata;
  assign rsp_rdata = sel_ppi_r ? ppi_icb_rsp_rdata : sel_clint_r ? clint_icb_rsp_rdata : sel_plic_r ? plic_icb_rsp_rdata : sel_fio_r ? fio_icb_rsp_rdata : mem_icb_rsp_rdata;
  logic rsp_ready;
  assign rsp_ready = sel_lsu_r ? lsu2biu_icb_rsp_ready : ifu2biu_icb_rsp_ready;
  assign biu_active = arb_valid;
  assign lsu2biu_icb_cmd_ready = lsu_win & arb_cmd_ready;
  assign ifu2biu_icb_cmd_ready = ~lsu_win & arb_cmd_ready;
  assign ppi_icb_cmd_valid = arb_valid & is_ppi;
  assign ppi_icb_cmd_addr = arb_addr;
  assign ppi_icb_cmd_read = arb_read;
  assign ppi_icb_cmd_wdata = arb_wdata;
  assign ppi_icb_cmd_wmask = arb_wmask;
  assign ppi_icb_cmd_burst = arb_burst;
  assign ppi_icb_cmd_beat = arb_beat;
  assign ppi_icb_cmd_lock = arb_lock;
  assign ppi_icb_cmd_excl = arb_excl;
  assign ppi_icb_cmd_size = arb_size;
  assign clint_icb_cmd_valid = arb_valid & is_clint;
  assign clint_icb_cmd_addr = arb_addr;
  assign clint_icb_cmd_read = arb_read;
  assign clint_icb_cmd_wdata = arb_wdata;
  assign clint_icb_cmd_wmask = arb_wmask;
  assign clint_icb_cmd_burst = arb_burst;
  assign clint_icb_cmd_beat = arb_beat;
  assign clint_icb_cmd_lock = arb_lock;
  assign clint_icb_cmd_excl = arb_excl;
  assign clint_icb_cmd_size = arb_size;
  assign plic_icb_cmd_valid = arb_valid & is_plic;
  assign plic_icb_cmd_addr = arb_addr;
  assign plic_icb_cmd_read = arb_read;
  assign plic_icb_cmd_wdata = arb_wdata;
  assign plic_icb_cmd_wmask = arb_wmask;
  assign plic_icb_cmd_burst = arb_burst;
  assign plic_icb_cmd_beat = arb_beat;
  assign plic_icb_cmd_lock = arb_lock;
  assign plic_icb_cmd_excl = arb_excl;
  assign plic_icb_cmd_size = arb_size;
  assign fio_icb_cmd_valid = arb_valid & is_fio;
  assign fio_icb_cmd_addr = arb_addr;
  assign fio_icb_cmd_read = arb_read;
  assign fio_icb_cmd_wdata = arb_wdata;
  assign fio_icb_cmd_wmask = arb_wmask;
  assign fio_icb_cmd_burst = arb_burst;
  assign fio_icb_cmd_beat = arb_beat;
  assign fio_icb_cmd_lock = arb_lock;
  assign fio_icb_cmd_excl = arb_excl;
  assign fio_icb_cmd_size = arb_size;
  assign mem_icb_cmd_valid = arb_valid & is_mem;
  assign mem_icb_cmd_addr = arb_addr;
  assign mem_icb_cmd_read = arb_read;
  assign mem_icb_cmd_wdata = arb_wdata;
  assign mem_icb_cmd_wmask = arb_wmask;
  assign mem_icb_cmd_burst = arb_burst;
  assign mem_icb_cmd_beat = arb_beat;
  assign mem_icb_cmd_lock = arb_lock;
  assign mem_icb_cmd_excl = arb_excl;
  assign mem_icb_cmd_size = arb_size;
  assign ppi_icb_rsp_ready = sel_ppi_r & rsp_ready;
  assign clint_icb_rsp_ready = sel_clint_r & rsp_ready;
  assign plic_icb_rsp_ready = sel_plic_r & rsp_ready;
  assign fio_icb_rsp_ready = sel_fio_r & rsp_ready;
  assign mem_icb_rsp_ready = sel_mem_r & rsp_ready;
  assign lsu2biu_icb_rsp_valid = sel_lsu_r & rsp_valid;
  assign lsu2biu_icb_rsp_err = rsp_err;
  assign lsu2biu_icb_rsp_excl_ok = rsp_excl_ok;
  assign lsu2biu_icb_rsp_rdata = rsp_rdata;
  assign ifu2biu_icb_rsp_valid = ~sel_lsu_r & rsp_valid;
  assign ifu2biu_icb_rsp_err = rsp_err;
  assign ifu2biu_icb_rsp_excl_ok = rsp_excl_ok;
  assign ifu2biu_icb_rsp_rdata = rsp_rdata;

endmodule

// Arbiter ready back to requestors
// Command to downstream ports
// Response ready to downstream
// Response to LSU/IFU
