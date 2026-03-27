// domain SysDomain

module TopModule (
  input logic clk,
  input logic ar,
  input logic d,
  output logic q
);

  logic q_r;
  always_ff @(posedge clk or posedge ar) begin
    if (ar) begin
      q_r <= 0;
    end else begin
      q_r <= d;
    end
  end
  assign q = q_r;

endmodule

