module dff #(
  parameter int N = 8
) (
  input logic clk,
  input logic reset,
  input logic [N-1:0] D,
  output logic [N-1:0] Q
);

  logic [N-1:0] Q_r;
  always_ff @(posedge clk) begin
    if (reset) begin
      Q_r <= 0;
    end else begin
      Q_r <= D;
    end
  end
  assign Q = Q_r;

endmodule

