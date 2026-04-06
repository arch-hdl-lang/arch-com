// E203 Outstanding Instruction Track FIFO (OITF)
// Tracks in-flight long-latency instructions for hazard detection.
// Circular FIFO with 2 entries; stores rd info + FPU flags.
// Matches RealBench port interface.
module e203_exu_oitf #(
  parameter int OITF_DEPTH = 2
) (
  input logic clk,
  input logic rst_n,
  output logic dis_ready,
  input logic dis_ena,
  input logic ret_ena,
  output logic [1-1:0] dis_ptr,
  output logic [1-1:0] ret_ptr,
  output logic [5-1:0] ret_rdidx,
  output logic ret_rdwen,
  output logic ret_rdfpu,
  output logic [32-1:0] ret_pc,
  input logic disp_i_rs1en,
  input logic disp_i_rs2en,
  input logic disp_i_rs3en,
  input logic disp_i_rdwen,
  input logic disp_i_rs1fpu,
  input logic disp_i_rs2fpu,
  input logic disp_i_rs3fpu,
  input logic disp_i_rdfpu,
  input logic [5-1:0] disp_i_rs1idx,
  input logic [5-1:0] disp_i_rs2idx,
  input logic [5-1:0] disp_i_rs3idx,
  input logic [5-1:0] disp_i_rdidx,
  input logic [32-1:0] disp_i_pc,
  output logic oitfrd_match_disprs1,
  output logic oitfrd_match_disprs2,
  output logic oitfrd_match_disprs3,
  output logic oitfrd_match_disprd,
  output logic oitf_empty
);

  // ── Dispatch interface ────────────────────────────────────────────
  // ── Pointer outputs ───────────────────────────────────────────────
  // ── Retire info outputs ───────────────────────────────────────────
  // ── Dispatch info inputs ──────────────────────────────────────────
  // ── Hazard check outputs ──────────────────────────────────────────
  // ── FIFO state registers ──────────────────────────────────────────
  logic valid_0 = 1'b0;
  logic valid_1 = 1'b0;
  logic [5-1:0] rdidx_0;
  logic [5-1:0] rdidx_1;
  logic rdwen_0;
  logic rdwen_1;
  logic rdfpu_0;
  logic rdfpu_1;
  logic [32-1:0] pc_0;
  logic [32-1:0] pc_1;
  logic wr_ptr_r = 1'b0;
  logic rd_ptr_r = 1'b0;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      rd_ptr_r <= 1'b0;
      valid_0 <= 1'b0;
      valid_1 <= 1'b0;
      wr_ptr_r <= 1'b0;
    end else begin
      // Allocate: write new entry at wr_ptr
      if (dis_ena & dis_ready) begin
        if (wr_ptr_r == 1'b0) begin
          valid_0 <= 1'b1;
          rdidx_0 <= disp_i_rdidx;
          rdwen_0 <= disp_i_rdwen;
          rdfpu_0 <= disp_i_rdfpu;
          pc_0 <= disp_i_pc;
        end else begin
          valid_1 <= 1'b1;
          rdidx_1 <= disp_i_rdidx;
          rdwen_1 <= disp_i_rdwen;
          rdfpu_1 <= disp_i_rdfpu;
          pc_1 <= disp_i_pc;
        end
        wr_ptr_r <= ~wr_ptr_r;
      end
      // Deallocate: clear oldest entry at rd_ptr
      if (ret_ena) begin
        if (rd_ptr_r == 1'b0) begin
          valid_0 <= 1'b0;
        end else begin
          valid_1 <= 1'b0;
        end
        rd_ptr_r <= ~rd_ptr_r;
      end
    end
  end
  always_comb begin
    // FIFO status
    oitf_empty = ~valid_0 & ~valid_1;
    dis_ready = ~(valid_0 & valid_1);
    // Pointer outputs
    dis_ptr = 1'($unsigned(wr_ptr_r));
    ret_ptr = 1'($unsigned(rd_ptr_r));
    // Return oldest entry info (at rd_ptr)
    if (rd_ptr_r == 1'b0) begin
      ret_rdidx = rdidx_0;
      ret_rdwen = rdwen_0;
      ret_rdfpu = rdfpu_0;
      ret_pc = pc_0;
    end else begin
      ret_rdidx = rdidx_1;
      ret_rdwen = rdwen_1;
      ret_rdfpu = rdfpu_1;
      ret_pc = pc_1;
    end
    // ── Hazard checks: compare dispatch rs/rd against all valid entries ──
    // Hazard checks: compare dispatch rs/rd against valid OITF entries
    // Match only when FPU types agree: both FPU or both integer
    oitfrd_match_disprs1 = valid_0 & rdwen_0 & disp_i_rs1en & disp_i_rs1idx == rdidx_0 & disp_i_rs1fpu == rdfpu_0 | valid_1 & rdwen_1 & disp_i_rs1en & disp_i_rs1idx == rdidx_1 & disp_i_rs1fpu == rdfpu_1;
    oitfrd_match_disprs2 = valid_0 & rdwen_0 & disp_i_rs2en & disp_i_rs2idx == rdidx_0 & disp_i_rs2fpu == rdfpu_0 | valid_1 & rdwen_1 & disp_i_rs2en & disp_i_rs2idx == rdidx_1 & disp_i_rs2fpu == rdfpu_1;
    oitfrd_match_disprs3 = valid_0 & rdwen_0 & disp_i_rs3en & disp_i_rs3idx == rdidx_0 & disp_i_rs3fpu == rdfpu_0 | valid_1 & rdwen_1 & disp_i_rs3en & disp_i_rs3idx == rdidx_1 & disp_i_rs3fpu == rdfpu_1;
    oitfrd_match_disprd = valid_0 & rdwen_0 & disp_i_rdwen & disp_i_rdidx == rdidx_0 & disp_i_rdfpu == rdfpu_0 | valid_1 & rdwen_1 & disp_i_rdwen & disp_i_rdidx == rdidx_1 & disp_i_rdfpu == rdfpu_1;
  end

endmodule

