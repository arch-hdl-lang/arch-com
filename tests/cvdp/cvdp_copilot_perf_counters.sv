module cvdp_copilot_perf_counters #(
  parameter int CNT_W = 10
) (
  input logic clk,
  input logic reset,
  input logic sw_req_i,
  input logic cpu_trig_i,
  output logic [CNT_W-1:0] p_count_o
);

  logic [CNT_W-1:0] count_next;
  logic [CNT_W-1:0] count_q;
  always_comb begin
    if (sw_req_i) begin
      if (cpu_trig_i) begin
        count_next = 1;
      end else begin
        count_next = 0;
      end
    end else if (cpu_trig_i) begin
      count_next = CNT_W'(count_q + 1);
    end else begin
      count_next = count_q;
    end
    if (sw_req_i) begin
      p_count_o = count_next;
    end else begin
      p_count_o = 0;
    end
  end
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      count_q <= 0;
    end else begin
      count_q <= count_next;
    end
  end

endmodule

