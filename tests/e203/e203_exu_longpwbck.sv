// E203 Long-Pipe Writeback Collector
// Collects results from long-latency units (MulDiv, LSU load)
// and arbitrates them into a single writeback port.
// Priority: LSU > MulDiv (loads are more latency-critical).
module ExuLongpWbck (
  input logic lsu_wbck_valid,
  output logic lsu_wbck_ready,
  input logic [32-1:0] lsu_wbck_wdat,
  input logic [5-1:0] lsu_wbck_rd_idx,
  input logic lsu_wbck_rd_en,
  input logic mdv_wbck_valid,
  output logic mdv_wbck_ready,
  input logic [32-1:0] mdv_wbck_wdat,
  input logic [5-1:0] mdv_wbck_rd_idx,
  input logic mdv_wbck_rd_en,
  output logic o_valid,
  input logic o_ready,
  output logic [32-1:0] o_wdat,
  output logic [5-1:0] o_rd_idx,
  output logic o_rd_en
);

  // LSU load result
  // MulDiv result
  // Merged output to ExuWbck long-pipe port
  // LSU wins when both valid (priority arbiter)
  logic lsu_win;
  assign lsu_win = lsu_wbck_valid;
  always_comb begin
    o_valid = (lsu_wbck_valid | mdv_wbck_valid);
    if (lsu_win) begin
      o_wdat = lsu_wbck_wdat;
      o_rd_idx = lsu_wbck_rd_idx;
      o_rd_en = lsu_wbck_rd_en;
    end else begin
      o_wdat = mdv_wbck_wdat;
      o_rd_idx = mdv_wbck_rd_idx;
      o_rd_en = mdv_wbck_rd_en;
    end
    lsu_wbck_ready = (lsu_win & o_ready);
    mdv_wbck_ready = ((~lsu_win) & o_ready);
  end

endmodule

// Handshake: grant to winner
