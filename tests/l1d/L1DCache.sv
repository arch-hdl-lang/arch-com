// L1D Cache — top-level integration.
// SETS=64, WAYS=8, LINE_BYTES=64, ADDR_W=64, DATA_W=64, TAG_W=52
// CPU-side: req/resp flat ports  |  Memory-side: AXI4 flat ports
module L1DCache (
  input logic clk,
  input logic rst,
  input logic req_valid,
  output logic req_ready,
  input logic [64-1:0] req_vaddr,
  input logic [64-1:0] req_data,
  input logic [8-1:0] req_be,
  input logic req_is_store,
  output logic resp_valid,
  output logic [64-1:0] resp_data,
  output logic resp_error,
  output logic ar_valid,
  input logic ar_ready,
  output logic [64-1:0] ar_addr,
  output logic [4-1:0] ar_id,
  output logic [8-1:0] ar_len,
  output logic [3-1:0] ar_size,
  output logic [2-1:0] ar_burst,
  input logic r_valid,
  output logic r_ready,
  input logic [64-1:0] r_data,
  input logic [4-1:0] r_id,
  input logic [2-1:0] r_resp,
  input logic r_last,
  output logic aw_valid,
  input logic aw_ready,
  output logic [64-1:0] aw_addr,
  output logic [4-1:0] aw_id,
  output logic [8-1:0] aw_len,
  output logic [3-1:0] aw_size,
  output logic [2-1:0] aw_burst,
  output logic w_valid,
  input logic w_ready,
  output logic [64-1:0] w_data,
  output logic [8-1:0] w_strb,
  output logic w_last,
  input logic b_valid,
  output logic b_ready,
  input logic [4-1:0] b_id,
  input logic [2-1:0] b_resp
);

  // ── CPU request ─────────────────────────────────────────────────────────────
  // ── CPU response ─────────────────────────────────────────────────────────────
  // ── AXI4 AR channel (fill reads) ─────────────────────────────────────────────
  // ── AXI4 R channel (fill data) ───────────────────────────────────────────────
  // ── AXI4 AW channel (writeback address) ──────────────────────────────────────
  // ── AXI4 W channel (writeback data) ──────────────────────────────────────────
  // ── AXI4 B channel (writeback response) ──────────────────────────────────────
  // ── Tag array wires — Vec per signal, one element per way ────────────────────
  logic tag_rd_en_w [8-1:0];
  logic [6-1:0] tag_rd_addr_w [8-1:0];
  logic [54-1:0] tag_rd_data_w [8-1:0];
  logic tag_wr_en_w [8-1:0];
  logic [6-1:0] tag_wr_addr_w [8-1:0];
  logic [54-1:0] tag_wr_data_w [8-1:0];
  // ── Data SRAM wires ──────────────────────────────────────────────────────────
  logic data_rd_en_w;
  logic [12-1:0] data_rd_addr_w;
  logic [64-1:0] data_rd_data_w;
  logic data_wr_en_w;
  logic [12-1:0] data_wr_addr_w;
  logic [64-1:0] data_wr_data_w;
  // ── LRU SRAM wires ───────────────────────────────────────────────────────────
  logic lru_rd_en_w;
  logic [6-1:0] lru_rd_addr_w;
  logic [7-1:0] lru_rd_data_w;
  logic lru_wr_en_w;
  logic [6-1:0] lru_wr_addr_w;
  logic [7-1:0] lru_wr_data_w;
  // ── LRU module wires ─────────────────────────────────────────────────────────
  logic [7-1:0] lru_tree_in_w;
  logic [3-1:0] lru_access_way_w;
  logic lru_access_en_w;
  logic [7-1:0] lru_tree_out_w;
  logic [3-1:0] lru_victim_way_w;
  // ── Fill / Writeback FSM interface wires ─────────────────────────────────────
  logic fill_start_w;
  logic [64-1:0] fill_addr_w;
  logic fill_done_w;
  logic [64-1:0] fill_word_w [8-1:0];
  logic wb_start_w;
  logic [64-1:0] wb_addr_w;
  logic wb_done_w;
  logic [64-1:0] wb_word_w [8-1:0];
  // ── CPU interface output wires ────────────────────────────────────────────────
  logic req_ready_w;
  logic resp_valid_w;
  logic [64-1:0] resp_data_w;
  logic resp_error_w;
  // ── AXI fill output wires ─────────────────────────────────────────────────────
  logic ar_valid_w;
  logic [64-1:0] ar_addr_w;
  logic [4-1:0] ar_id_w;
  logic [8-1:0] ar_len_w;
  logic [3-1:0] ar_size_w;
  logic [2-1:0] ar_burst_w;
  logic r_ready_w;
  // ── AXI writeback output wires ────────────────────────────────────────────────
  logic aw_valid_w;
  logic [64-1:0] aw_addr_w;
  logic [4-1:0] aw_id_w;
  logic [8-1:0] aw_len_w;
  logic [3-1:0] aw_size_w;
  logic [2-1:0] aw_burst_w;
  logic w_valid_w;
  logic [64-1:0] w_data_w;
  logic [8-1:0] w_strb_w;
  logic w_last_w;
  logic b_ready_w;
  // ── Tag arrays — 8 ways via generate for ─────────────────────────────────────
  RamTagArray tag_0 (
    .clk(clk),
    .rd_port_en(tag_rd_en_w[0]),
    .rd_port_addr(tag_rd_addr_w[0]),
    .rd_port_rdata(tag_rd_data_w[0]),
    .wr_port_en(tag_wr_en_w[0]),
    .wr_port_addr(tag_wr_addr_w[0]),
    .wr_port_wdata(tag_wr_data_w[0])
  );
  RamTagArray tag_1 (
    .clk(clk),
    .rd_port_en(tag_rd_en_w[1]),
    .rd_port_addr(tag_rd_addr_w[1]),
    .rd_port_rdata(tag_rd_data_w[1]),
    .wr_port_en(tag_wr_en_w[1]),
    .wr_port_addr(tag_wr_addr_w[1]),
    .wr_port_wdata(tag_wr_data_w[1])
  );
  RamTagArray tag_2 (
    .clk(clk),
    .rd_port_en(tag_rd_en_w[2]),
    .rd_port_addr(tag_rd_addr_w[2]),
    .rd_port_rdata(tag_rd_data_w[2]),
    .wr_port_en(tag_wr_en_w[2]),
    .wr_port_addr(tag_wr_addr_w[2]),
    .wr_port_wdata(tag_wr_data_w[2])
  );
  RamTagArray tag_3 (
    .clk(clk),
    .rd_port_en(tag_rd_en_w[3]),
    .rd_port_addr(tag_rd_addr_w[3]),
    .rd_port_rdata(tag_rd_data_w[3]),
    .wr_port_en(tag_wr_en_w[3]),
    .wr_port_addr(tag_wr_addr_w[3]),
    .wr_port_wdata(tag_wr_data_w[3])
  );
  RamTagArray tag_4 (
    .clk(clk),
    .rd_port_en(tag_rd_en_w[4]),
    .rd_port_addr(tag_rd_addr_w[4]),
    .rd_port_rdata(tag_rd_data_w[4]),
    .wr_port_en(tag_wr_en_w[4]),
    .wr_port_addr(tag_wr_addr_w[4]),
    .wr_port_wdata(tag_wr_data_w[4])
  );
  RamTagArray tag_5 (
    .clk(clk),
    .rd_port_en(tag_rd_en_w[5]),
    .rd_port_addr(tag_rd_addr_w[5]),
    .rd_port_rdata(tag_rd_data_w[5]),
    .wr_port_en(tag_wr_en_w[5]),
    .wr_port_addr(tag_wr_addr_w[5]),
    .wr_port_wdata(tag_wr_data_w[5])
  );
  RamTagArray tag_6 (
    .clk(clk),
    .rd_port_en(tag_rd_en_w[6]),
    .rd_port_addr(tag_rd_addr_w[6]),
    .rd_port_rdata(tag_rd_data_w[6]),
    .wr_port_en(tag_wr_en_w[6]),
    .wr_port_addr(tag_wr_addr_w[6]),
    .wr_port_wdata(tag_wr_data_w[6])
  );
  RamTagArray tag_7 (
    .clk(clk),
    .rd_port_en(tag_rd_en_w[7]),
    .rd_port_addr(tag_rd_addr_w[7]),
    .rd_port_rdata(tag_rd_data_w[7]),
    .wr_port_en(tag_wr_en_w[7]),
    .wr_port_addr(tag_wr_addr_w[7]),
    .wr_port_wdata(tag_wr_data_w[7])
  );
  // ── Data array ────────────────────────────────────────────────────────────────
  RamDataArray data_ram (
    .clk(clk),
    .rd_port_en(data_rd_en_w),
    .rd_port_addr(data_rd_addr_w),
    .rd_port_rdata(data_rd_data_w),
    .wr_port_en(data_wr_en_w),
    .wr_port_addr(data_wr_addr_w),
    .wr_port_wdata(data_wr_data_w)
  );
  // ── LRU state array ───────────────────────────────────────────────────────────
  RamLruState lru_ram (
    .clk(clk),
    .rd_port_en(lru_rd_en_w),
    .rd_port_addr(lru_rd_addr_w),
    .rd_port_rdata(lru_rd_data_w),
    .wr_port_en(lru_wr_en_w),
    .wr_port_addr(lru_wr_addr_w),
    .wr_port_wdata(lru_wr_data_w)
  );
  // ── LRU update module ─────────────────────────────────────────────────────────
  ModuleLruUpdate lru_upd (
    .tree_in(lru_tree_in_w),
    .access_way(lru_access_way_w),
    .access_en(lru_access_en_w),
    .tree_out(lru_tree_out_w),
    .victim_way(lru_victim_way_w)
  );
  // ── Cache controller ──────────────────────────────────────────────────────────
  FsmCacheCtrl ctrl (
    .clk(clk),
    .rst(rst),
    .req_valid(req_valid),
    .req_ready(req_ready_w),
    .req_vaddr(req_vaddr),
    .req_data(req_data),
    .req_be(req_be),
    .req_is_store(req_is_store),
    .resp_valid(resp_valid_w),
    .resp_data(resp_data_w),
    .resp_error(resp_error_w),
    .tag_rd_en(tag_rd_en_w),
    .tag_rd_addr(tag_rd_addr_w),
    .tag_rd_data(tag_rd_data_w),
    .tag_wr_en(tag_wr_en_w),
    .tag_wr_addr(tag_wr_addr_w),
    .tag_wr_data(tag_wr_data_w),
    .data_rd_en(data_rd_en_w),
    .data_rd_addr(data_rd_addr_w),
    .data_rd_data(data_rd_data_w),
    .data_wr_en(data_wr_en_w),
    .data_wr_addr(data_wr_addr_w),
    .data_wr_data(data_wr_data_w),
    .lru_rd_en(lru_rd_en_w),
    .lru_rd_addr(lru_rd_addr_w),
    .lru_rd_data(lru_rd_data_w),
    .lru_wr_en(lru_wr_en_w),
    .lru_wr_addr(lru_wr_addr_w),
    .lru_wr_data(lru_wr_data_w),
    .lru_tree_in(lru_tree_in_w),
    .lru_access_way(lru_access_way_w),
    .lru_access_en(lru_access_en_w),
    .lru_tree_out(lru_tree_out_w),
    .lru_victim_way(lru_victim_way_w),
    .fill_start(fill_start_w),
    .fill_addr(fill_addr_w),
    .fill_done(fill_done_w),
    .fill_word(fill_word_w),
    .wb_start(wb_start_w),
    .wb_addr(wb_addr_w),
    .wb_done(wb_done_w),
    .wb_word(wb_word_w)
  );
  // CPU interface
  // Tag SRAM (Vec ports — one connection per Vec signal)
  // Data SRAM
  // LRU SRAM
  // LRU module
  // Fill FSM
  // Writeback FSM
  // ── Fill FSM ─────────────────────────────────────────────────────────────────
  FsmAxi4Fill fill_fsm (
    .clk(clk),
    .rst(rst),
    .fill_start(fill_start_w),
    .fill_addr(fill_addr_w),
    .fill_done(fill_done_w),
    .fill_word(fill_word_w),
    .ar_valid(ar_valid_w),
    .ar_ready(ar_ready),
    .ar_addr(ar_addr_w),
    .ar_id(ar_id_w),
    .ar_len(ar_len_w),
    .ar_size(ar_size_w),
    .ar_burst(ar_burst_w),
    .r_valid(r_valid),
    .r_ready(r_ready_w),
    .r_data(r_data),
    .r_id(r_id),
    .r_resp(r_resp),
    .r_last(r_last)
  );
  // ── Writeback FSM ─────────────────────────────────────────────────────────────
  FsmAxi4Wb wb_fsm (
    .clk(clk),
    .rst(rst),
    .wb_start(wb_start_w),
    .wb_addr(wb_addr_w),
    .wb_done(wb_done_w),
    .wb_word(wb_word_w),
    .aw_valid(aw_valid_w),
    .aw_ready(aw_ready),
    .aw_addr(aw_addr_w),
    .aw_id(aw_id_w),
    .aw_len(aw_len_w),
    .aw_size(aw_size_w),
    .aw_burst(aw_burst_w),
    .w_valid(w_valid_w),
    .w_ready(w_ready),
    .w_data(w_data_w),
    .w_strb(w_strb_w),
    .w_last(w_last_w),
    .b_valid(b_valid),
    .b_ready(b_ready_w),
    .b_id(b_id),
    .b_resp(b_resp)
  );
  // ── Route wires to top-level output ports ────────────────────────────────────
  assign req_ready = req_ready_w;
  assign resp_valid = resp_valid_w;
  assign resp_data = resp_data_w;
  assign resp_error = resp_error_w;
  assign ar_valid = ar_valid_w;
  assign ar_addr = ar_addr_w;
  assign ar_id = ar_id_w;
  assign ar_len = ar_len_w;
  assign ar_size = ar_size_w;
  assign ar_burst = ar_burst_w;
  assign r_ready = r_ready_w;
  assign aw_valid = aw_valid_w;
  assign aw_addr = aw_addr_w;
  assign aw_id = aw_id_w;
  assign aw_len = aw_len_w;
  assign aw_size = aw_size_w;
  assign aw_burst = aw_burst_w;
  assign w_valid = w_valid_w;
  assign w_data = w_data_w;
  assign w_strb = w_strb_w;
  assign w_last = w_last_w;
  assign b_ready = b_ready_w;

endmodule

