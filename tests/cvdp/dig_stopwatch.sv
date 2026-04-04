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
  logic prev_start;
  logic [6-1:0] r_seconds;
  logic [6-1:0] r_minutes;
  logic [1-1:0] r_hour;
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      clk_cnt <= 0;
      one_sec_pulse <= 1'b0;
      prev_start <= 1'b0;
      r_hour <= 0;
      r_minutes <= 0;
      r_seconds <= 0;
    end else begin
      one_sec_pulse <= 1'b0;
      prev_start <= start_stop;
      // Clock divider: advance counter while running.
      if (start_stop) begin
        if (clk_cnt == 32'($unsigned(CLK_FREQ - 1))) begin
          clk_cnt <= 0;
          one_sec_pulse <= 1'b1;
        end else begin
          clk_cnt <= 32'(clk_cnt + 1);
        end
      end
      // Counter updates: use (start_stop | prev_start) so that a pending
      // pulse from the clock when start_stop dropped still counts.
      if (one_sec_pulse & (start_stop | prev_start) & ~r_hour) begin
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
  // Combinational outputs: lookahead when pulse is active and running
  // (or was running one cycle ago via prev_start).
  always_comb begin
    if (one_sec_pulse & (start_stop | prev_start) & ~r_hour) begin
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

