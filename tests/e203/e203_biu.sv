// domain SysDomain
//   freq_mhz: 100

// domain SysDomain
//   freq_mhz: 100

// domain SysDomain
//   freq_mhz: 100

// domain SysDomain
//   freq_mhz: 100

// domain SysDomain
//   freq_mhz: 100

// domain SysDomain
//   freq_mhz: 100

// domain SysDomain
//   freq_mhz: 100

module e203_biu (
  input logic clk,
  input logic rst_n,
  output logic biu_active,
  input logic lsu2biu_icb_cmd_valid,
  output logic lsu2biu_icb_cmd_ready,
  input logic [31:0] lsu2biu_icb_cmd_addr,
  input logic lsu2biu_icb_cmd_read,
  input logic [31:0] lsu2biu_icb_cmd_wdata,
  input logic [3:0] lsu2biu_icb_cmd_wmask,
  input logic [1:0] lsu2biu_icb_cmd_burst,
  input logic [1:0] lsu2biu_icb_cmd_beat,
  input logic lsu2biu_icb_cmd_lock,
  input logic lsu2biu_icb_cmd_excl,
  input logic [1:0] lsu2biu_icb_cmd_size,
  output logic lsu2biu_icb_rsp_valid,
  input logic lsu2biu_icb_rsp_ready,
  output logic lsu2biu_icb_rsp_err,
  output logic lsu2biu_icb_rsp_excl_ok,
  output logic [31:0] lsu2biu_icb_rsp_rdata,
  input logic ifu2biu_icb_cmd_valid,
  output logic ifu2biu_icb_cmd_ready,
  input logic [31:0] ifu2biu_icb_cmd_addr,
  input logic ifu2biu_icb_cmd_read,
  input logic [31:0] ifu2biu_icb_cmd_wdata,
  input logic [3:0] ifu2biu_icb_cmd_wmask,
  input logic [1:0] ifu2biu_icb_cmd_burst,
  input logic [1:0] ifu2biu_icb_cmd_beat,
  input logic ifu2biu_icb_cmd_lock,
  input logic ifu2biu_icb_cmd_excl,
  input logic [1:0] ifu2biu_icb_cmd_size,
  output logic ifu2biu_icb_rsp_valid,
  input logic ifu2biu_icb_rsp_ready,
  output logic ifu2biu_icb_rsp_err,
  output logic ifu2biu_icb_rsp_excl_ok,
  output logic [31:0] ifu2biu_icb_rsp_rdata,
  input logic [31:0] ppi_region_indic,
  input logic ppi_icb_enable,
  output logic ppi_icb_cmd_valid,
  input logic ppi_icb_cmd_ready,
  output logic [31:0] ppi_icb_cmd_addr,
  output logic ppi_icb_cmd_read,
  output logic [31:0] ppi_icb_cmd_wdata,
  output logic [3:0] ppi_icb_cmd_wmask,
  output logic [1:0] ppi_icb_cmd_burst,
  output logic [1:0] ppi_icb_cmd_beat,
  output logic ppi_icb_cmd_lock,
  output logic ppi_icb_cmd_excl,
  output logic [1:0] ppi_icb_cmd_size,
  input logic ppi_icb_rsp_valid,
  output logic ppi_icb_rsp_ready,
  input logic ppi_icb_rsp_err,
  input logic ppi_icb_rsp_excl_ok,
  input logic [31:0] ppi_icb_rsp_rdata,
  input logic [31:0] clint_region_indic,
  input logic clint_icb_enable,
  output logic clint_icb_cmd_valid,
  input logic clint_icb_cmd_ready,
  output logic [31:0] clint_icb_cmd_addr,
  output logic clint_icb_cmd_read,
  output logic [31:0] clint_icb_cmd_wdata,
  output logic [3:0] clint_icb_cmd_wmask,
  output logic [1:0] clint_icb_cmd_burst,
  output logic [1:0] clint_icb_cmd_beat,
  output logic clint_icb_cmd_lock,
  output logic clint_icb_cmd_excl,
  output logic [1:0] clint_icb_cmd_size,
  input logic clint_icb_rsp_valid,
  output logic clint_icb_rsp_ready,
  input logic clint_icb_rsp_err,
  input logic clint_icb_rsp_excl_ok,
  input logic [31:0] clint_icb_rsp_rdata,
  input logic [31:0] plic_region_indic,
  input logic plic_icb_enable,
  output logic plic_icb_cmd_valid,
  input logic plic_icb_cmd_ready,
  output logic [31:0] plic_icb_cmd_addr,
  output logic plic_icb_cmd_read,
  output logic [31:0] plic_icb_cmd_wdata,
  output logic [3:0] plic_icb_cmd_wmask,
  output logic [1:0] plic_icb_cmd_burst,
  output logic [1:0] plic_icb_cmd_beat,
  output logic plic_icb_cmd_lock,
  output logic plic_icb_cmd_excl,
  output logic [1:0] plic_icb_cmd_size,
  input logic plic_icb_rsp_valid,
  output logic plic_icb_rsp_ready,
  input logic plic_icb_rsp_err,
  input logic plic_icb_rsp_excl_ok,
  input logic [31:0] plic_icb_rsp_rdata,
  input logic [31:0] fio_region_indic,
  input logic fio_icb_enable,
  output logic fio_icb_cmd_valid,
  input logic fio_icb_cmd_ready,
  output logic [31:0] fio_icb_cmd_addr,
  output logic fio_icb_cmd_read,
  output logic [31:0] fio_icb_cmd_wdata,
  output logic [3:0] fio_icb_cmd_wmask,
  output logic [1:0] fio_icb_cmd_burst,
  output logic [1:0] fio_icb_cmd_beat,
  output logic fio_icb_cmd_lock,
  output logic fio_icb_cmd_excl,
  output logic [1:0] fio_icb_cmd_size,
  input logic fio_icb_rsp_valid,
  output logic fio_icb_rsp_ready,
  input logic fio_icb_rsp_err,
  input logic fio_icb_rsp_excl_ok,
  input logic [31:0] fio_icb_rsp_rdata,
  input logic mem_icb_enable,
  output logic mem_icb_cmd_valid,
  input logic mem_icb_cmd_ready,
  output logic [31:0] mem_icb_cmd_addr,
  output logic mem_icb_cmd_read,
  output logic [31:0] mem_icb_cmd_wdata,
  output logic [3:0] mem_icb_cmd_wmask,
  output logic [1:0] mem_icb_cmd_burst,
  output logic [1:0] mem_icb_cmd_beat,
  output logic mem_icb_cmd_lock,
  output logic mem_icb_cmd_excl,
  output logic [1:0] mem_icb_cmd_size,
  input logic mem_icb_rsp_valid,
  output logic mem_icb_rsp_ready,
  input logic mem_icb_rsp_err,
  input logic mem_icb_rsp_excl_ok,
  input logic [31:0] mem_icb_rsp_rdata
);

  logic lsu_win;
  logic ifu_req;
  logic arb_valid;
  logic [31:0] arb_addr;
  logic arb_read;
  logic [31:0] arb_wdata;
  logic [3:0] arb_wmask;
  logic [1:0] arb_burst;
  logic [1:0] arb_beat;
  logic arb_lock;
  logic arb_excl;
  logic [1:0] arb_size;
  logic is_ppi;
  logic is_clint;
  logic is_plic;
  logic is_fio;
  logic is_mem;
  logic ifu_access;
  logic ifu_to_peri;
  logic arb_cmd_ready;
  logic cmd_accept;
  logic downstream_cmd_ready;
  logic out_flag_set;
  logic out_flag_clr;
  logic can_accept;
  logic rsp_valid_from_target;
  logic rsp_err_from_target;
  logic rsp_excl_ok_from_target;
  logic [31:0] rsp_rdata_from_target;
  logic rsp_ready_from_initiator;
  assign lsu_win = lsu2biu_icb_cmd_valid;
  assign ifu_req = ifu2biu_icb_cmd_valid;
  assign arb_valid = lsu_win | ifu_req;
  assign arb_addr = lsu_win ? lsu2biu_icb_cmd_addr : ifu2biu_icb_cmd_addr;
  assign arb_read = lsu_win ? lsu2biu_icb_cmd_read : ifu2biu_icb_cmd_read;
  assign arb_wdata = lsu_win ? lsu2biu_icb_cmd_wdata : ifu2biu_icb_cmd_wdata;
  assign arb_wmask = lsu_win ? lsu2biu_icb_cmd_wmask : ifu2biu_icb_cmd_wmask;
  assign arb_burst = lsu_win ? lsu2biu_icb_cmd_burst : ifu2biu_icb_cmd_burst;
  assign arb_beat = lsu_win ? lsu2biu_icb_cmd_beat : ifu2biu_icb_cmd_beat;
  assign arb_lock = lsu_win ? lsu2biu_icb_cmd_lock : ifu2biu_icb_cmd_lock;
  assign arb_excl = lsu_win ? lsu2biu_icb_cmd_excl : ifu2biu_icb_cmd_excl;
  assign arb_size = lsu_win ? lsu2biu_icb_cmd_size : ifu2biu_icb_cmd_size;
  assign is_ppi = ppi_icb_enable & (arb_addr[31:16] == ppi_region_indic[31:16]);
  assign is_clint = clint_icb_enable & (arb_addr[31:16] == clint_region_indic[31:16]);
  assign is_plic = plic_icb_enable & (arb_addr[31:16] == plic_region_indic[31:16]);
  assign is_fio = fio_icb_enable & (arb_addr[31:16] == fio_region_indic[31:16]);
  assign is_mem = mem_icb_enable & ~is_ppi & ~is_clint & ~is_plic & ~is_fio;
  assign ifu_access = ~lsu_win & arb_valid;
  assign ifu_to_peri = ifu_access & ~is_mem;
  assign arb_cmd_ready = (is_ppi & ppi_icb_cmd_ready) | (is_clint & clint_icb_cmd_ready) | (is_plic & plic_icb_cmd_ready) | (is_fio & fio_icb_cmd_ready) | (is_mem & mem_icb_cmd_ready);
  logic cmd_valid_r;
  logic [31:0] cmd_addr_r;
  logic cmd_read_r;
  logic [31:0] cmd_wdata_r;
  logic [3:0] cmd_wmask_r;
  logic [1:0] cmd_burst_r;
  logic [1:0] cmd_beat_r;
  logic cmd_lock_r;
  logic cmd_excl_r;
  logic [1:0] cmd_size_r;
  logic tgt_lsu_r;
  logic tgt_ppi_r;
  logic tgt_clint_r;
  logic tgt_plic_r;
  logic tgt_fio_r;
  logic tgt_mem_r;
  assign cmd_accept = arb_valid & arb_cmd_ready & ~ifu_to_peri;
  assign downstream_cmd_ready = (tgt_ppi_r & ppi_icb_cmd_ready) | (tgt_clint_r & clint_icb_cmd_ready) | (tgt_plic_r & plic_icb_cmd_ready) | (tgt_fio_r & fio_icb_cmd_ready) | (tgt_mem_r & mem_icb_cmd_ready);
  logic out_flag_r;
  assign out_flag_set = cmd_accept;
  assign out_flag_clr = rsp_valid_from_target & rsp_ready_from_initiator;
  assign can_accept = ~cmd_valid_r | downstream_cmd_ready;
  assign rsp_valid_from_target = (tgt_ppi_r & ppi_icb_rsp_valid) | (tgt_clint_r & clint_icb_rsp_valid) | (tgt_plic_r & plic_icb_rsp_valid) | (tgt_fio_r & fio_icb_rsp_valid) | (tgt_mem_r & mem_icb_rsp_valid);
  assign rsp_err_from_target = (tgt_ppi_r & ppi_icb_rsp_err) | (tgt_clint_r & clint_icb_rsp_err) | (tgt_plic_r & plic_icb_rsp_err) | (tgt_fio_r & fio_icb_rsp_err) | (tgt_mem_r & mem_icb_rsp_err);
  assign rsp_excl_ok_from_target = (tgt_ppi_r & ppi_icb_rsp_excl_ok) | (tgt_clint_r & clint_icb_rsp_excl_ok) | (tgt_plic_r & plic_icb_rsp_excl_ok) | (tgt_fio_r & fio_icb_rsp_excl_ok) | (tgt_mem_r & mem_icb_rsp_excl_ok);
  assign rsp_rdata_from_target = tgt_ppi_r ? ppi_icb_rsp_rdata : tgt_clint_r ? clint_icb_rsp_rdata : tgt_plic_r ? plic_icb_rsp_rdata : tgt_fio_r ? fio_icb_rsp_rdata : mem_icb_rsp_rdata;
  assign rsp_ready_from_initiator = tgt_lsu_r ? lsu2biu_icb_rsp_ready : ifu2biu_icb_rsp_ready;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      cmd_addr_r <= 0;
      cmd_beat_r <= 0;
      cmd_burst_r <= 0;
      cmd_excl_r <= 1'b0;
      cmd_lock_r <= 1'b0;
      cmd_read_r <= 1'b0;
      cmd_size_r <= 0;
      cmd_valid_r <= 1'b0;
      cmd_wdata_r <= 0;
      cmd_wmask_r <= 0;
      tgt_clint_r <= 1'b0;
      tgt_fio_r <= 1'b0;
      tgt_lsu_r <= 1'b0;
      tgt_mem_r <= 1'b0;
      tgt_plic_r <= 1'b0;
      tgt_ppi_r <= 1'b0;
    end else begin
      if (cmd_accept) begin
        cmd_valid_r <= 1'b1;
        cmd_addr_r <= arb_addr;
        cmd_read_r <= arb_read;
        cmd_wdata_r <= arb_wdata;
        cmd_wmask_r <= arb_wmask;
        cmd_burst_r <= arb_burst;
        cmd_beat_r <= arb_beat;
        cmd_lock_r <= arb_lock;
        cmd_excl_r <= arb_excl;
        cmd_size_r <= arb_size;
        tgt_lsu_r <= lsu_win;
        tgt_ppi_r <= is_ppi;
        tgt_clint_r <= is_clint;
        tgt_plic_r <= is_plic;
        tgt_fio_r <= is_fio;
        tgt_mem_r <= is_mem;
      end else if (cmd_valid_r & downstream_cmd_ready) begin
        cmd_valid_r <= 1'b0;
      end
    end
  end
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      out_flag_r <= 1'b0;
    end else begin
      if (out_flag_set) begin
        out_flag_r <= 1'b1;
      end else if (out_flag_clr) begin
        out_flag_r <= 1'b0;
      end
    end
  end
  always_comb begin
    biu_active = cmd_valid_r | out_flag_r | arb_valid;
    if (lsu_win) begin
      lsu2biu_icb_cmd_ready = can_accept & arb_cmd_ready & ~ifu_to_peri;
      ifu2biu_icb_cmd_ready = 1'b0;
    end else begin
      lsu2biu_icb_cmd_ready = 1'b0;
      ifu2biu_icb_cmd_ready = can_accept & arb_cmd_ready & ~ifu_to_peri;
    end
    ppi_icb_cmd_valid = cmd_valid_r & tgt_ppi_r;
    ppi_icb_cmd_addr = tgt_ppi_r ? cmd_addr_r : 0;
    ppi_icb_cmd_read = tgt_ppi_r ? cmd_read_r : 1'b0;
    ppi_icb_cmd_wdata = tgt_ppi_r ? cmd_wdata_r : 0;
    ppi_icb_cmd_wmask = tgt_ppi_r ? cmd_wmask_r : 0;
    ppi_icb_cmd_burst = tgt_ppi_r ? cmd_burst_r : 0;
    ppi_icb_cmd_beat = tgt_ppi_r ? cmd_beat_r : 0;
    ppi_icb_cmd_lock = tgt_ppi_r ? cmd_lock_r : 1'b0;
    ppi_icb_cmd_excl = tgt_ppi_r ? cmd_excl_r : 1'b0;
    ppi_icb_cmd_size = tgt_ppi_r ? cmd_size_r : 0;
    clint_icb_cmd_valid = cmd_valid_r & tgt_clint_r;
    clint_icb_cmd_addr = tgt_clint_r ? cmd_addr_r : 0;
    clint_icb_cmd_read = tgt_clint_r ? cmd_read_r : 1'b0;
    clint_icb_cmd_wdata = tgt_clint_r ? cmd_wdata_r : 0;
    clint_icb_cmd_wmask = tgt_clint_r ? cmd_wmask_r : 0;
    clint_icb_cmd_burst = tgt_clint_r ? cmd_burst_r : 0;
    clint_icb_cmd_beat = tgt_clint_r ? cmd_beat_r : 0;
    clint_icb_cmd_lock = tgt_clint_r ? cmd_lock_r : 1'b0;
    clint_icb_cmd_excl = tgt_clint_r ? cmd_excl_r : 1'b0;
    clint_icb_cmd_size = tgt_clint_r ? cmd_size_r : 0;
    plic_icb_cmd_valid = cmd_valid_r & tgt_plic_r;
    plic_icb_cmd_addr = tgt_plic_r ? cmd_addr_r : 0;
    plic_icb_cmd_read = tgt_plic_r ? cmd_read_r : 1'b0;
    plic_icb_cmd_wdata = tgt_plic_r ? cmd_wdata_r : 0;
    plic_icb_cmd_wmask = tgt_plic_r ? cmd_wmask_r : 0;
    plic_icb_cmd_burst = tgt_plic_r ? cmd_burst_r : 0;
    plic_icb_cmd_beat = tgt_plic_r ? cmd_beat_r : 0;
    plic_icb_cmd_lock = tgt_plic_r ? cmd_lock_r : 1'b0;
    plic_icb_cmd_excl = tgt_plic_r ? cmd_excl_r : 1'b0;
    plic_icb_cmd_size = tgt_plic_r ? cmd_size_r : 0;
    fio_icb_cmd_valid = cmd_valid_r & tgt_fio_r;
    fio_icb_cmd_addr = tgt_fio_r ? cmd_addr_r : 0;
    fio_icb_cmd_read = tgt_fio_r ? cmd_read_r : 1'b0;
    fio_icb_cmd_wdata = tgt_fio_r ? cmd_wdata_r : 0;
    fio_icb_cmd_wmask = tgt_fio_r ? cmd_wmask_r : 0;
    fio_icb_cmd_burst = tgt_fio_r ? cmd_burst_r : 0;
    fio_icb_cmd_beat = tgt_fio_r ? cmd_beat_r : 0;
    fio_icb_cmd_lock = tgt_fio_r ? cmd_lock_r : 1'b0;
    fio_icb_cmd_excl = tgt_fio_r ? cmd_excl_r : 1'b0;
    fio_icb_cmd_size = tgt_fio_r ? cmd_size_r : 0;
    mem_icb_cmd_valid = cmd_valid_r & tgt_mem_r;
    mem_icb_cmd_addr = tgt_mem_r ? cmd_addr_r : 0;
    mem_icb_cmd_read = tgt_mem_r ? cmd_read_r : 1'b0;
    mem_icb_cmd_wdata = tgt_mem_r ? cmd_wdata_r : 0;
    mem_icb_cmd_wmask = tgt_mem_r ? cmd_wmask_r : 0;
    mem_icb_cmd_burst = tgt_mem_r ? cmd_burst_r : 0;
    mem_icb_cmd_beat = tgt_mem_r ? cmd_beat_r : 0;
    mem_icb_cmd_lock = tgt_mem_r ? cmd_lock_r : 1'b0;
    mem_icb_cmd_excl = tgt_mem_r ? cmd_excl_r : 1'b0;
    mem_icb_cmd_size = tgt_mem_r ? cmd_size_r : 0;
    ppi_icb_rsp_ready = tgt_ppi_r & rsp_ready_from_initiator;
    clint_icb_rsp_ready = tgt_clint_r & rsp_ready_from_initiator;
    plic_icb_rsp_ready = tgt_plic_r & rsp_ready_from_initiator;
    fio_icb_rsp_ready = tgt_fio_r & rsp_ready_from_initiator;
    mem_icb_rsp_ready = tgt_mem_r & rsp_ready_from_initiator;
    if (ifu_to_peri) begin
      lsu2biu_icb_rsp_valid = 1'b0;
      lsu2biu_icb_rsp_err = 1'b0;
      lsu2biu_icb_rsp_excl_ok = 1'b0;
      lsu2biu_icb_rsp_rdata = 0;
      ifu2biu_icb_rsp_valid = arb_valid;
      ifu2biu_icb_rsp_err = 1'b1;
      ifu2biu_icb_rsp_excl_ok = 1'b0;
      ifu2biu_icb_rsp_rdata = 0;
    end else begin
      lsu2biu_icb_rsp_valid = tgt_lsu_r & rsp_valid_from_target;
      lsu2biu_icb_rsp_err = rsp_err_from_target;
      lsu2biu_icb_rsp_excl_ok = rsp_excl_ok_from_target;
      lsu2biu_icb_rsp_rdata = rsp_rdata_from_target;
      ifu2biu_icb_rsp_valid = ~tgt_lsu_r & rsp_valid_from_target;
      ifu2biu_icb_rsp_err = rsp_err_from_target;
      ifu2biu_icb_rsp_excl_ok = rsp_excl_ok_from_target;
      ifu2biu_icb_rsp_rdata = rsp_rdata_from_target;
    end
  end

endmodule

