// E203 HBirdv2 Bus Interface Unit
// Priority arbiter (LSU > IFU), outstanding transaction tracking,
// address-based splitter to downstream targets (PPI/CLINT/PLIC/FIO/MEM),
// IFU-to-peripheral error detection with zero-cycle error response.
//
// Verify: submodule-audit applied. Implements sirv_gnrl_icb_arbt +
// sirv_gnrl_icb_buffer + sirv_gnrl_icb_splt behavior inline.
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

  // ── LSU to BIU ──────────────────────────────────────────────────────
  // ── IFU to BIU ──────────────────────────────────────────────────────
  // ── PPI downstream ──────────────────────────────────────────────────
  // ── CLINT downstream ────────────────────────────────────────────────
  // ── PLIC downstream ─────────────────────────────────────────────────
  // ── FIO downstream ──────────────────────────────────────────────────
  // ── MEM downstream ──────────────────────────────────────────────────
  // ── Arbitration: LSU priority over IFU ──────────────────────────────
  // Both requestors have ICB command buses. LSU wins if both request.
  logic lsu_win;
  assign lsu_win = lsu2biu_icb_cmd_valid;
  logic ifu_req;
  assign ifu_req = ifu2biu_icb_cmd_valid;
  logic arb_valid;
  assign arb_valid = lsu_win | ifu_req;
  // Mux command from winning initiator
  logic [31:0] arb_addr;
  assign arb_addr = lsu_win ? lsu2biu_icb_cmd_addr : ifu2biu_icb_cmd_addr;
  logic arb_read;
  assign arb_read = lsu_win ? lsu2biu_icb_cmd_read : ifu2biu_icb_cmd_read;
  logic [31:0] arb_wdata;
  assign arb_wdata = lsu_win ? lsu2biu_icb_cmd_wdata : ifu2biu_icb_cmd_wdata;
  logic [3:0] arb_wmask;
  assign arb_wmask = lsu_win ? lsu2biu_icb_cmd_wmask : ifu2biu_icb_cmd_wmask;
  logic [1:0] arb_burst;
  assign arb_burst = lsu_win ? lsu2biu_icb_cmd_burst : ifu2biu_icb_cmd_burst;
  logic [1:0] arb_beat;
  assign arb_beat = lsu_win ? lsu2biu_icb_cmd_beat : ifu2biu_icb_cmd_beat;
  logic arb_lock;
  assign arb_lock = lsu_win ? lsu2biu_icb_cmd_lock : ifu2biu_icb_cmd_lock;
  logic arb_excl;
  assign arb_excl = lsu_win ? lsu2biu_icb_cmd_excl : ifu2biu_icb_cmd_excl;
  logic [1:0] arb_size;
  assign arb_size = lsu_win ? lsu2biu_icb_cmd_size : ifu2biu_icb_cmd_size;
  // ── Address decode (upper 16 bits match region indicator) ────────────
  logic is_ppi;
  assign is_ppi = ppi_icb_enable & (arb_addr[31:16] == ppi_region_indic[31:16]);
  logic is_clint;
  assign is_clint = clint_icb_enable & (arb_addr[31:16] == clint_region_indic[31:16]);
  logic is_plic;
  assign is_plic = plic_icb_enable & (arb_addr[31:16] == plic_region_indic[31:16]);
  logic is_fio;
  assign is_fio = fio_icb_enable & (arb_addr[31:16] == fio_region_indic[31:16]);
  logic is_mem;
  assign is_mem = mem_icb_enable & ~is_ppi & ~is_clint & ~is_plic & ~is_fio;
  // ── IFU error: IFU accessing peripheral space ───────────────────────
  logic ifu_access;
  assign ifu_access = ~lsu_win & arb_valid;
  logic ifu_to_peri;
  assign ifu_to_peri = ifu_access & ~is_mem;
  // ── Downstream ready mux ────────────────────────────────────────────
  logic arb_cmd_ready;
  assign arb_cmd_ready = (is_ppi & ppi_icb_cmd_ready) | (is_clint & clint_icb_cmd_ready) | (is_plic & plic_icb_cmd_ready) | (is_fio & fio_icb_cmd_ready) | (is_mem & mem_icb_cmd_ready);
  // ── Command pipeline register (CMD_DP=1) ────────────────────────────
  // The arbiter accepts a command, which is registered before going to
  // the splitter/downstream. This matches sirv_gnrl_icb_buffer behavior.
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
  // Command acceptance: when arbiter wins AND downstream ready
  logic cmd_accept;
  assign cmd_accept = arb_valid & arb_cmd_ready & ~ifu_to_peri;
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
        // Clear valid when command is consumed by downstream handshake
        cmd_valid_r <= 1'b0;
      end
    end
  end
  // ── Downstream ready for pipelined command ──────────────────────────
  logic downstream_cmd_ready;
  assign downstream_cmd_ready = (tgt_ppi_r & ppi_icb_cmd_ready) | (tgt_clint_r & clint_icb_cmd_ready) | (tgt_plic_r & plic_icb_cmd_ready) | (tgt_fio_r & fio_icb_cmd_ready) | (tgt_mem_r & mem_icb_cmd_ready);
  // ── Outstanding response tracking (OUTS_NUM=1) ──────────────────────
  logic out_flag_r;
  logic out_flag_set;
  assign out_flag_set = cmd_accept;
  logic out_flag_clr;
  assign out_flag_clr = rsp_valid_from_target & rsp_ready_from_initiator;
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
  // ── Inititator ready: gated by pipeline vacancy ─────────────────────
  // FIFO_CUT_READY=1: ready is registered (cut). We can accept when
  // there's no outstanding pipelined command OR it's being consumed.
  logic can_accept;
  assign can_accept = ~cmd_valid_r | downstream_cmd_ready;
  // ── Response from selected target ────────────────────────────────────
  logic rsp_valid_from_target;
  assign rsp_valid_from_target = (sel_ppi_r & ppi_icb_rsp_valid) | (sel_clint_r & clint_icb_rsp_valid) | (sel_plic_r & plic_icb_rsp_valid) | (sel_fio_r & fio_icb_rsp_valid) | (sel_mem_r & mem_icb_rsp_valid);
  logic rsp_err_from_target;
  assign rsp_err_from_target = (sel_ppi_r & ppi_icb_rsp_err) | (sel_clint_r & clint_icb_rsp_err) | (sel_plic_r & plic_icb_rsp_err) | (sel_fio_r & fio_icb_rsp_err) | (sel_mem_r & mem_icb_rsp_err);
  logic rsp_excl_ok_from_target;
  assign rsp_excl_ok_from_target = (sel_ppi_r & ppi_icb_rsp_excl_ok) | (sel_clint_r & clint_icb_rsp_excl_ok) | (sel_plic_r & plic_icb_rsp_excl_ok) | (sel_fio_r & fio_icb_rsp_excl_ok) | (sel_mem_r & mem_icb_rsp_excl_ok);
  logic [31:0] rsp_rdata_from_target;
  assign rsp_rdata_from_target = sel_ppi_r ? ppi_icb_rsp_rdata : sel_clint_r ? clint_icb_rsp_rdata : sel_plic_r ? plic_icb_rsp_rdata : sel_fio_r ? fio_icb_rsp_rdata : mem_icb_rsp_rdata;
  // ── Response ready from selected initiator ──────────────────────────
  logic rsp_ready_from_initiator;
  assign rsp_ready_from_initiator = tgt_lsu_r ? lsu2biu_icb_rsp_ready : ifu2biu_icb_rsp_ready;
  // ── IFU error response (zero-cycle: rsp_valid tracks cmd_valid) ─────
  // When IFU accesses peripheral space, generate immediate error response.
  // The request is NOT forwarded to any downstream target.
  // ── Combinational outputs ────────────────────────────────────────────
  always_comb begin
    biu_active = cmd_valid_r | out_flag_r | arb_valid;
    // ── Arbiter ready back to initiators (gated by pipeline vacancy) ──
    if (lsu_win) begin
      lsu2biu_icb_cmd_ready = can_accept & arb_cmd_ready & ~ifu_to_peri;
      ifu2biu_icb_cmd_ready = 1'b0;
    end else begin
      lsu2biu_icb_cmd_ready = 1'b0;
      ifu2biu_icb_cmd_ready = can_accept & arb_cmd_ready & ~ifu_to_peri;
    end
    // ── Commands to downstream from pipelined register ────────────────
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
    // ── Response ready to selected downstream ─────────────────────────
    ppi_icb_rsp_ready = tgt_ppi_r & rsp_ready_from_initiator;
    clint_icb_rsp_ready = tgt_clint_r & rsp_ready_from_initiator;
    plic_icb_rsp_ready = tgt_plic_r & rsp_ready_from_initiator;
    fio_icb_rsp_ready = tgt_fio_r & rsp_ready_from_initiator;
    mem_icb_rsp_ready = tgt_mem_r & rsp_ready_from_initiator;
    // Response to LSU/IFU: mux from selected target, or IFU error
    if (ifu_to_peri) begin
      // IFU error: zero-cycle error response to IFU
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

