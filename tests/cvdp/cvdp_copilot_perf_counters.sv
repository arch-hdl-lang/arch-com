module cvdp_copilot_perf_counters (
  input logic clk,
  input logic reset,
  input logic cpu_trig_i,
  input logic sw_req_i,
  output logic [7:0] p_count_o
);

  logic [7:0] cnt;
  always_ff @(posedge clk) begin
    if (reset) begin
      cnt <= 0;
    end else begin
      if (sw_req_i) begin
        cnt <= 8'($unsigned(cpu_trig_i));
      end else if (cpu_trig_i) begin
        cnt <= 8'(cnt + 8'd1);
      end
    end
  end
  logic [7:0] p_count_comb;
  always_comb begin
    if (sw_req_i) begin
      p_count_comb = cnt;
    end else begin
      p_count_comb = 0;
    end
  end
  assign p_count_o = p_count_comb;

endmodule

