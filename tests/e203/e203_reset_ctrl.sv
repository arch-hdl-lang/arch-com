// E203 Reset Controller
// 2-stage reset release synchronizer with test_mode bypass.
// MASTER=1: after async rst_n deasserts, outputs stay low for 2 clk cycles
// before releasing (going high). test_mode bypasses synchronizer.
// MASTER=0 (slave/lockstep): just pass rst_n through directly.
module e203_reset_ctrl #(
  parameter int MASTER = 1
) (
  input logic clk,
  input logic rst_n,
  input logic test_mode,
  output logic rst_core,
  output logic rst_itcm,
  output logic rst_dtcm,
  output logic rst_aon
);

  // Reset release status (active-high = not in reset)
  // 2-stage reset synchronizer shift register
  // After rst_n deasserts, shifts in 1s: 00 -> 01 -> 11
  logic rst_sync_r0;
  logic rst_sync_r1;
  logic rst_sync_n;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      rst_sync_r0 <= 1'b0;
      rst_sync_r1 <= 1'b0;
    end else begin
      // Only shift in master mode; in slave mode regs are unused
      rst_sync_r0 <= 1'b1;
      rst_sync_r1 <= rst_sync_r0;
    end
  end
  always_comb begin
    // MASTER=1: in test_mode bypass synchronizer, else use rst_sync_r1
    // MASTER=0: just pass rst_n through directly
    if (MASTER > 0) begin
      rst_sync_n = test_mode ? rst_n : rst_sync_r1;
    end else begin
      rst_sync_n = rst_n;
    end
    rst_core = rst_sync_n;
    rst_itcm = rst_sync_n;
    rst_dtcm = rst_sync_n;
    rst_aon = rst_sync_n;
  end

endmodule

