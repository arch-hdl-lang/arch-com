module dig_stopwatch #(
  parameter int CLK_FREQ = 50000000
) (
  input logic clk,
  input logic reset,
  input logic start_stop,
  output logic [6-1:0] seconds,
  output logic [6-1:0] minutes,
  output logic [1-1:0] hour
);

  logic [32-1:0] clk_cnt;
  logic one_sec_pulse;
  logic [6-1:0] r_seconds;
  logic [6-1:0] r_minutes;
  logic [1-1:0] r_hour;
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      clk_cnt <= 0;
      one_sec_pulse <= 1'b0;
      r_hour <= 0;
      r_minutes <= 0;
      r_seconds <= 0;
    end else begin
      one_sec_pulse <= 1'b0;
      // Clock divider keeps running after hour=1 so cocotb can still
      // await RisingEdge(one_sec_pulse) in the test loop.
      if (start_stop) begin
        if (clk_cnt == 32'($unsigned(CLK_FREQ - 1))) begin
          clk_cnt <= 0;
          one_sec_pulse <= 1'b1;
        end else begin
          clk_cnt <= 32'(clk_cnt + 1);
        end
      end
      // Counter updates (gated by ~r_hour to stop at 1:00:00)
      if (one_sec_pulse & start_stop & ~r_hour) begin
        if (r_seconds == 59) begin
          r_seconds <= 0;
          if (r_minutes == 59) begin
            r_minutes <= 0;
            r_hour <= 1;
          end else begin
            r_minutes <= 6'(r_minutes + 1);
          end
        end else begin
          r_seconds <= 6'(r_seconds + 1);
        end
      end
    end
  end
  // Combinational outputs: show next-state when pulse is active,
  // so cocotb sees updated values at RisingEdge(clk) before NBA.
  always_comb begin
    if (one_sec_pulse & start_stop & ~r_hour) begin
      if (r_seconds == 59) begin
        seconds = 0;
        if (r_minutes == 59) begin
          minutes = 0;
          hour = 1;
        end else begin
          minutes = 6'(r_minutes + 1);
          hour = r_hour;
        end
      end else begin
        seconds = 6'(r_seconds + 1);
        minutes = r_minutes;
        hour = r_hour;
      end
    end else begin
      seconds = r_seconds;
      minutes = r_minutes;
      hour = r_hour;
    end
  end

endmodule

