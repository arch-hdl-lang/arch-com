// E203 Outstanding Instruction Track FIFO (OITF)
// Tracks in-flight long-latency instructions (MulDiv, LSU) for hazard detection.
// Circular FIFO with 2 entries; each entry stores destination register index
// and a "has rd" flag.
//
// On dispatch of a long-pipe op: allocate (push) entry with rd info.
// On long-pipe writeback: deallocate (pop) oldest entry.
// Hazard check: compare new instruction's rs1/rs2/rd against all valid entries.
module ExuOitf #(
  parameter int OITF_DEPTH = 2
) (
  input logic clk,
  input logic rst_n,
  input logic dis_ena,
  input logic [5-1:0] dis_rd_idx,
  input logic dis_rd_en,
  output logic dis_ready,
  input logic ret_ena,
  output logic [5-1:0] ret_rd_idx,
  output logic ret_rd_en,
  input logic [5-1:0] chk_rs1_idx,
  input logic chk_rs1_en,
  input logic [5-1:0] chk_rs2_idx,
  input logic chk_rs2_en,
  input logic [5-1:0] chk_rd_idx,
  input logic chk_rd_en,
  output logic raw_dep,
  output logic waw_dep,
  output logic dep_stall,
  output logic oitf_empty
);

  // ── Dispatch interface (allocate on long-pipe dispatch) ────────────
  // ── Writeback interface (deallocate on long-pipe completion) ────────
  // ── Hazard check inputs (from decode of new instruction) ───────────
  // ── Hazard outputs ─────────────────────────────────────────────────
  // ── FIFO state ─────────────────────────────────────────────────────
  logic valid_0 = 1'b0;
  logic valid_1 = 1'b0;
  logic [5-1:0] rdidx_0 = 0;
  logic [5-1:0] rdidx_1 = 0;
  logic rden_0 = 1'b0;
  logic rden_1 = 1'b0;
  logic wr_ptr = 1'b0;
  logic rd_ptr = 1'b0;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      rd_ptr <= 1'b0;
      rden_0 <= 1'b0;
      rden_1 <= 1'b0;
      rdidx_0 <= 0;
      rdidx_1 <= 0;
      valid_0 <= 1'b0;
      valid_1 <= 1'b0;
      wr_ptr <= 1'b0;
    end else begin
      if ((dis_ena & dis_ready)) begin
        if ((wr_ptr == 1'b0)) begin
          valid_0 <= 1'b1;
          rdidx_0 <= dis_rd_idx;
          rden_0 <= dis_rd_en;
        end else begin
          valid_1 <= 1'b1;
          rdidx_1 <= dis_rd_idx;
          rden_1 <= dis_rd_en;
        end
        wr_ptr <= (~wr_ptr);
      end
      if (ret_ena) begin
        if ((rd_ptr == 1'b0)) begin
          valid_0 <= 1'b0;
        end else begin
          valid_1 <= 1'b0;
        end
        rd_ptr <= (~rd_ptr);
      end
    end
  end
  // Allocate: write new entry at wr_ptr
  // Deallocate: clear oldest entry at rd_ptr
  always_comb begin
    oitf_empty = ((~valid_0) & (~valid_1));
    dis_ready = (~(valid_0 & valid_1));
    if ((rd_ptr == 1'b0)) begin
      ret_rd_idx = rdidx_0;
      ret_rd_en = rden_0;
    end else begin
      ret_rd_idx = rdidx_1;
      ret_rd_en = rden_1;
    end
    raw_dep = (((valid_0 & rden_0) & ((chk_rs1_en & (chk_rs1_idx == rdidx_0)) | (chk_rs2_en & (chk_rs2_idx == rdidx_0)))) | ((valid_1 & rden_1) & ((chk_rs1_en & (chk_rs1_idx == rdidx_1)) | (chk_rs2_en & (chk_rs2_idx == rdidx_1)))));
    waw_dep = ((((valid_0 & rden_0) & chk_rd_en) & (chk_rd_idx == rdidx_0)) | (((valid_1 & rden_1) & chk_rd_en) & (chk_rd_idx == rdidx_1)));
    dep_stall = (raw_dep | waw_dep);
  end

endmodule

// FIFO status
// Return oldest entry info (at rd_ptr)
// ── RAW dependency: new rs1/rs2 matches any valid entry's rd ─────
// ── WAW dependency: new rd matches any valid entry's rd ──────────
// Stall dispatch if any dependency exists
