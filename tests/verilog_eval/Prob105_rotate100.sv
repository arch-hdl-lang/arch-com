module TopModule (
  input logic clk,
  input logic load,
  input logic [2-1:0] ena,
  input logic [100-1:0] data,
  output logic [100-1:0] q
);

  always_ff @(posedge clk) begin
    if (load) begin
      q <= data;
    end else if (ena == 1) begin
      q <= {q[0], q[99:1]};
    end else if (ena == 2) begin
      q <= {q[98:0], q[99]};
    end
  end

endmodule

