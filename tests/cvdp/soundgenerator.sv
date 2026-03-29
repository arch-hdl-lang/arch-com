module strob_gen__PERIOD_US_1000 #(
  parameter int CLOCK_HZ = 10000000,
  parameter int PERIOD_US = 100,
  parameter int DELAY = CLOCK_HZ * PERIOD_US / 1000000 - 1
) (
  input logic clk,
  input logic nrst,
  input logic enable,
  output logic strobe_o
);

  // DELAY = (CLOCK_HZ * PERIOD_US / 1_000_000) - 1
  logic [32-1:0] cnt;
  always_ff @(posedge clk or negedge nrst) begin
    if ((!nrst)) begin
      cnt <= DELAY;
      strobe_o <= 1'b0;
    end else begin
      if (cnt == 0) begin
        if (enable == 1'b1) begin
          strobe_o <= 1'b1;
        end else begin
          strobe_o <= 1'b0;
        end
        cnt <= DELAY;
      end else begin
        strobe_o <= 1'b0;
        if (enable == 1'b1) begin
          cnt <= 32'(cnt - 1);
        end else begin
          cnt <= DELAY;
        end
      end
    end
  end

endmodule

module strob_gen__PERIOD_US_1 #(
  parameter int CLOCK_HZ = 10000000,
  parameter int PERIOD_US = 100,
  parameter int DELAY = CLOCK_HZ * PERIOD_US / 1000000 - 1
) (
  input logic clk,
  input logic nrst,
  input logic enable,
  output logic strobe_o
);

  logic [32-1:0] cnt;
  always_ff @(posedge clk or negedge nrst) begin
    if ((!nrst)) begin
      cnt <= DELAY;
      strobe_o <= 1'b0;
    end else begin
      if (cnt == 0) begin
        if (enable == 1'b1) begin
          strobe_o <= 1'b1;
        end else begin
          strobe_o <= 1'b0;
        end
        cnt <= DELAY;
      end else begin
        strobe_o <= 1'b0;
        if (enable == 1'b1) begin
          cnt <= 32'(cnt - 1);
        end else begin
          cnt <= DELAY;
        end
      end
    end
  end

endmodule

module soundgenerator #(
  parameter int CLOCK_HZ = 10000000
) (
  input logic clk,
  input logic nrst,
  input logic start,
  input logic finish,
  input logic [16-1:0] sond_dur_ms_i,
  input logic [16-1:0] half_period_us_i,
  output logic soundwave_o,
  output logic busy,
  output logic done
);

  // Internal registers
  logic [16-1:0] duration_cnt;
  logic [16-1:0] halfperiodtimer;
  logic signal_r;
  logic busy_d;
  // Wires for strobe outputs
  logic TickMilli;
  logic tickmicro;
  logic busy_w;
  // Strobe generators
  strob_gen__PERIOD_US_1000 #(.CLOCK_HZ(CLOCK_HZ), .PERIOD_US(1000)) u_tick_milli (
    .clk(clk),
    .nrst(nrst),
    .enable(busy_w),
    .strobe_o(TickMilli)
  );
  strob_gen__PERIOD_US_1 #(.CLOCK_HZ(CLOCK_HZ), .PERIOD_US(1)) u_tick_micro (
    .clk(clk),
    .nrst(nrst),
    .enable(busy_w),
    .strobe_o(tickmicro)
  );
  // busy when duration counter is non-zero
  assign busy_w = duration_cnt != 0;
  // Combinational outputs
  always_comb begin
    busy = busy_w;
    done = busy_d & ~busy_w;
    if (busy_w == 1'b1) begin
      soundwave_o = signal_r;
    end else begin
      soundwave_o = 1'b0;
    end
  end
  // Sequential logic
  always_ff @(posedge clk or negedge nrst) begin
    if ((!nrst)) begin
      busy_d <= 1'b0;
      duration_cnt <= 0;
      halfperiodtimer <= 0;
      signal_r <= 1'b0;
    end else begin
      busy_d <= busy_w;
      if (start == 1'b1) begin
        duration_cnt <= sond_dur_ms_i;
        halfperiodtimer <= half_period_us_i;
        signal_r <= 1'b0;
      end else if (finish == 1'b1) begin
        duration_cnt <= 0;
        signal_r <= 1'b0;
      end else begin
        // Duration countdown
        if (TickMilli == 1'b1) begin
          if (duration_cnt != 0) begin
            duration_cnt <= 16'(duration_cnt - 1);
          end
        end
        // Half-period countdown for square wave
        if (tickmicro == 1'b1) begin
          if (busy_w == 1'b1) begin
            if (halfperiodtimer == 0) begin
              halfperiodtimer <= half_period_us_i;
              signal_r <= signal_r ^ 1'b1;
            end else begin
              halfperiodtimer <= 16'(halfperiodtimer - 1);
            end
          end
        end
      end
    end
  end

endmodule

