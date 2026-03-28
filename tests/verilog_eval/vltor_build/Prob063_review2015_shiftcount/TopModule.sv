// VerilogEval Prob063: 4-bit shift register / down counter
// Shift: data enters at LSB, shifts left: q <= {q[2:0], data}
module TopModule (
  input logic clk,
  input logic shift_ena,
  input logic count_ena,
  input logic data,
  output logic [4-1:0] q
);

  always_ff @(posedge clk) begin
    if (shift_ena) begin
      q[0] <= data;
      q[1] <= q[0];
      q[2] <= q[1];
      q[3] <= q[2];
    end else if (count_ena) begin
      q <= 4'(q - 1);
    end
  end

endmodule

