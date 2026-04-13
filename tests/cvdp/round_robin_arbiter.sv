module round_robin_arbiter #(
  parameter int N = 4,
  parameter int TIMEOUT = 16
) (
  input logic clk,
  input logic rstn,
  input logic [N-1:0] req,
  input logic [N-1:0] priority_level,
  output logic [N-1:0] grant,
  output logic idle
);

  // Per-channel timeout counters (Vec of 32-bit)
  logic [N-1:0] [31:0] tout_cnt;
  // Round-robin pointer: last granted channel (one-hot)
  logic [N-1:0] last_grant_idx;
  // Timeout flags: channel timed out if counter >= TIMEOUT
  logic [N-1:0] timed_out;
  always_comb begin
    timed_out = 0;
    for (int i = 0; i <= N - 1; i++) begin
      if (tout_cnt[i] >= 32'($unsigned(TIMEOUT))) begin
        timed_out = timed_out | N'($unsigned(1)) << $clog2(N)'(i);
      end
    end
  end
  // Effective priority: original priority OR timed-out elevation
  logic [N-1:0] eff_prio;
  assign eff_prio = priority_level | timed_out;
  // High-priority requesting channels
  logic [N-1:0] hi_req;
  assign hi_req = req & eff_prio;
  // Low-priority requesting channels
  logic [N-1:0] lo_req;
  assign lo_req = req & ~eff_prio;
  // Active request set: prefer high-priority, fallback to low
  logic [N-1:0] active_req;
  assign active_req = hi_req != 0 ? hi_req : lo_req;
  // Round-robin masking: mask off channels at or below last granted
  logic [N-1:0] upper_mask;
  always_comb begin
    upper_mask = 0;
    for (int i = 0; i <= N - 1; i++) begin
      if (N'($unsigned(1)) << $clog2(N)'(i) > last_grant_idx) begin
        upper_mask = upper_mask | N'($unsigned(1)) << $clog2(N)'(i);
      end
    end
  end
  // Masked requests: only channels above last granted
  logic [N-1:0] masked_req;
  assign masked_req = active_req & upper_mask;
  // If masked set is empty, use full active set (wrap around)
  logic [N-1:0] rr_req;
  assign rr_req = masked_req != 0 ? masked_req : active_req;
  // Isolate lowest set bit: x & (-x) = x & (~x + 1)
  logic [N-1:0] rr_neg;
  assign rr_neg = N'(~rr_req + N'($unsigned(1)));
  logic [N-1:0] grant_sel;
  assign grant_sel = rr_req & rr_neg;
  // Final grant: only if there are active requests
  assign grant = req != 0 ? grant_sel : N'($unsigned(0));
  // Idle output
  assign idle = req == 0;
  // Update last_grant_idx and timeout counters
  always_ff @(posedge clk) begin
    if ((!rstn)) begin
      last_grant_idx <= 0;
      for (int __ri0 = 0; __ri0 < N; __ri0++) begin
        tout_cnt[__ri0] <= 0;
      end
    end else begin
      if (req != 0) begin
        last_grant_idx <= grant_sel;
      end
      for (int i = 0; i <= N - 1; i++) begin
        if (grant[i]) begin
          tout_cnt[i] <= 0;
        end else if (req[i]) begin
          tout_cnt[i] <= 32'(tout_cnt[i] + 1);
        end else begin
          tout_cnt[i] <= 0;
        end
      end
    end
  end

endmodule

