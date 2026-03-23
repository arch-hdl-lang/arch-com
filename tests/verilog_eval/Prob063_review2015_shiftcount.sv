// VerilogEval Prob063: 4-bit shift register / down counter
// Shift: data enters at LSB, shifts left: q <= {q[2:0], data}
// domain SysDomain

module TopModule (
  input logic clk,
  input logic shift_ena,
  input logic count_ena,
  input logic data,
  output logic [4-1:0] q
);

  logic [4-1:0] sr;
  always_ff @(posedge clk) begin
    if (shift_ena) begin
      sr[0] <= data;
      sr[1] <= sr[0];
      sr[2] <= sr[1];
      sr[3] <= sr[2];
    end else if (count_ena) begin
      sr <= 4'((sr - 1));
    end
  end
  assign q = sr;

endmodule

