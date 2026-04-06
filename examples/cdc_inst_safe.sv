// domain DomainA
//   freq_mhz: 100

// domain DomainB
//   freq_mhz: 200

module Producer (
  input logic clk,
  input logic rst,
  output logic [8-1:0] data_out
);

  logic [8-1:0] cnt = 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      cnt <= 0;
    end else begin
      cnt <= 8'(cnt + 1);
    end
  end
  assign data_out = cnt;

endmodule

module Consumer (
  input logic clk,
  input logic rst,
  input logic [8-1:0] data_in,
  output logic [8-1:0] data_out
);

  logic [8-1:0] r = 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      r <= 0;
    end else begin
      r <= data_in;
    end
  end
  assign data_out = r;

endmodule

module TopSafe (
  input logic clk_a,
  input logic clk_b,
  input logic rst,
  output logic [8-1:0] result
);

  logic [8-1:0] synced_data = 0;
  Producer prod (
    .clk(clk_a),
    .rst(rst),
    .data_out(synced_data)
  );
  Consumer cons (
    .clk(clk_b),
    .rst(rst),
    .data_in(synced_data),
    .data_out(result)
  );

endmodule

