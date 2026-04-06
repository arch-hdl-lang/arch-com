// domain FastDomain
//   freq_mhz: 200

// domain SlowDomain
//   freq_mhz: 50

module MultiClockSync (
  input logic fast_clk,
  input logic slow_clk,
  input logic rst,
  input logic [8-1:0] data_in,
  output logic [8-1:0] data_out,
  output logic [8-1:0] fast_count
);

  logic [8-1:0] fast_r = 0;
  logic [8-1:0] slow_r = 0;
  always_ff @(posedge fast_clk) begin
    if (rst) begin
      fast_r <= 0;
    end else begin
      fast_r <= 8'(fast_r + 1);
    end
  end
  always_ff @(posedge slow_clk) begin
    if (rst) begin
      slow_r <= 0;
    end else begin
      slow_r <= data_in;
    end
  end
  assign data_out = slow_r;
  assign fast_count = fast_r;

endmodule

