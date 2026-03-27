// domain SysDomain

module TopModule (
  input logic clk,
  input logic load,
  input logic [2-1:0] ena,
  input logic [100-1:0] data,
  output logic [100-1:0] q
);

  logic [100-1:0] q_r;
  always_ff @(posedge clk) begin
    if (load) begin
      q_r <= data;
    end else if (ena == 1) begin
      q_r <= {q_r[0], q_r[99:1]};
    end else if (ena == 2) begin
      q_r <= {q_r[98:0], q_r[99]};
    end
  end
  assign q = q_r;

endmodule

