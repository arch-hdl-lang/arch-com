// Test: override reset sync/async at instantiation time via `as Reset<...>`.
// The sub-module declares a concrete reset type; the parent overrides it at
// the connection site without changing the sub-module definition.
module GenCounter__rst_Sync_High (
  input logic clk,
  input logic rst,
  input logic en,
  output logic [7:0] count
);

  logic [7:0] count_r = 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      count_r <= 0;
    end else begin
      if (en) begin
        count_r <= 8'(count_r + 1);
      end
    end
  end
  assign count = count_r;

endmodule

module GenCounter__rst_Async_Low (
  input logic clk,
  input logic rst,
  input logic en,
  output logic [7:0] count
);

  logic [7:0] count_r = 0;
  always_ff @(posedge clk or negedge rst) begin
    if ((!rst)) begin
      count_r <= 0;
    end else begin
      if (en) begin
        count_r <= 8'(count_r + 1);
      end
    end
  end
  assign count = count_r;

endmodule

module ParamResetTop (
  input logic clk,
  input logic rst_sync,
  input logic rst_async_n,
  input logic en_a,
  input logic en_b,
  output logic [7:0] count_a,
  output logic [7:0] count_b
);

  logic [7:0] cnt_a;
  logic [7:0] cnt_b;
  // Instance A: use the default Sync reset (no override needed)
  GenCounter__rst_Sync_High sync_inst (
    .clk(clk),
    .rst(rst_sync),
    .en(en_a),
    .count(cnt_a)
  );
  // Instance B: override to Async, active-low at the connection site
  GenCounter__rst_Async_Low async_inst (
    .clk(clk),
    .rst(logic'(rst_async_n)),
    .en(en_b),
    .count(cnt_b)
  );
  assign count_a = cnt_a;
  assign count_b = cnt_b;

endmodule

