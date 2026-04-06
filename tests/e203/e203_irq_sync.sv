// E203 IRQ Synchronizer
// 2-stage FF synchronizer for async interrupt inputs.
// MASTER=1: synchronize through 2-FF chain; MASTER=0: pass-through.
module e203_irq_sync #(
  parameter int MASTER = 1
) (
  input logic clk,
  input logic rst_n,
  input logic ext_irq_a,
  input logic sft_irq_a,
  input logic tmr_irq_a,
  input logic dbg_irq_a,
  output logic ext_irq_r,
  output logic sft_irq_r,
  output logic tmr_irq_r,
  output logic dbg_irq_r
);

  // 2-stage synchronizer registers (stage 0 samples input, stage 1 = output)
  logic ext_s0;
  logic ext_s1;
  logic sft_s0;
  logic sft_s1;
  logic tmr_s0;
  logic tmr_s1;
  logic dbg_s0;
  logic dbg_s1;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      dbg_s0 <= 1'b0;
      dbg_s1 <= 1'b0;
      ext_s0 <= 1'b0;
      ext_s1 <= 1'b0;
      sft_s0 <= 1'b0;
      sft_s1 <= 1'b0;
      tmr_s0 <= 1'b0;
      tmr_s1 <= 1'b0;
    end else begin
      ext_s0 <= ext_irq_a;
      ext_s1 <= ext_s0;
      sft_s0 <= sft_irq_a;
      sft_s1 <= sft_s0;
      tmr_s0 <= tmr_irq_a;
      tmr_s1 <= tmr_s0;
      dbg_s0 <= dbg_irq_a;
      dbg_s1 <= dbg_s0;
    end
  end
  assign ext_irq_r = ext_s1;
  assign sft_irq_r = sft_s1;
  assign tmr_irq_r = tmr_s1;
  assign dbg_irq_r = dbg_s1;

endmodule

