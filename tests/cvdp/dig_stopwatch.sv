module dig_stopwatch #(
  parameter int CLK_FREQ = 50000000
) (
  input logic clk,
  input logic reset,
  input logic start_stop,
  output logic [5:0] seconds,
  output logic [5:0] minutes,
  output logic [7:0] hour,
  output logic one_sec_pulse
);

  logic [31:0] clk_cnt;
  logic [5:0] secs;
  logic [5:0] mins;
  logic [7:0] hrs;
  logic pulse;
  // cnt_wrap: combinational — fires when counter is at max and running
  logic cnt_wrap;
  assign cnt_wrap = start_stop & clk_cnt == 32'($unsigned(CLK_FREQ - 1));
  assign seconds = secs;
  assign minutes = mins;
  assign hour = hrs;
  assign one_sec_pulse = pulse;
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      clk_cnt <= 0;
      hrs <= 0;
      mins <= 0;
      pulse <= 0;
      secs <= 0;
    end else begin
      // pulse is registered copy of cnt_wrap; one_sec_pulse goes high cycle AFTER wrap
      pulse <= cnt_wrap;
      if (start_stop) begin
        if (cnt_wrap) begin
          clk_cnt <= 0;
        end else begin
          clk_cnt <= 32'(clk_cnt + 1);
        end
      end
      // Increment counters cycle AFTER pulse fires.
      // Gate with start_stop so a pending pulse is discarded if stopped.
      if (pulse & start_stop) begin
        if (secs == 59) begin
          secs <= 0;
          if (mins == 59) begin
            mins <= 0;
            hrs <= 8'(hrs + 1);
          end else begin
            mins <= 6'(mins + 1);
          end
        end else begin
          secs <= 6'(secs + 1);
        end
      end
    end
  end

endmodule

