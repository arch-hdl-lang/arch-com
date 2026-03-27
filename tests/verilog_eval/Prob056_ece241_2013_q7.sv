module TopModule (
  input logic clk,
  input logic j,
  input logic k,
  output logic Q
);

  always_ff @(posedge clk) begin
    if (j & k) begin
      Q <= ~Q;
    end else if (j) begin
      Q <= 1;
    end else if (k) begin
      Q <= 0;
    end
  end

endmodule

