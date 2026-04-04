// Test: `init on rst.asserted` block in seq.
// Allows polarity-independent, complex reset logic (e.g. Vec resets via for loops).
// Sync/async and polarity are determined by the reset port declaration —
// and can be overridden at instantiation via `as Reset<Async, Low>`.
// domain SysDomain
//   freq_mhz: 100

// Active-high sync reset with Vec initialization
module SyncHighInitOn (
  input logic clk,
  input logic rst,
  input logic en,
  output logic [8-1:0] out0,
  output logic [8-1:0] out1,
  output logic [8-1:0] out2,
  output logic [8-1:0] out3
);

  logic [8-1:0] table [4-1:0];
  always_ff @(posedge clk) begin
    if (rst) begin
      for (int i = 0; i <= 3; i++) begin
        table[i] <= 8'(i);
      end
    end
    if (en) begin
      table[0] <= 8'(table[0] + 1);
    end
  end
  assign out0 = table[0];
  assign out1 = table[1];
  assign out2 = table[2];
  assign out3 = table[3];

endmodule

// Active-low async reset with Vec initialization
module AsyncLowInitOn (
  input logic clk,
  input logic rst_n,
  input logic en,
  output logic [8-1:0] out0,
  output logic [8-1:0] out1,
  output logic [8-1:0] out2,
  output logic [8-1:0] out3
);

  logic [8-1:0] table [4-1:0];
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      for (int i = 0; i <= 3; i++) begin
        table[i] <= 8'(i);
      end
    end
    if (en) begin
      table[0] <= 8'(table[0] + 1);
    end
  end
  assign out0 = table[0];
  assign out1 = table[1];
  assign out2 = table[2];
  assign out3 = table[3];

endmodule

