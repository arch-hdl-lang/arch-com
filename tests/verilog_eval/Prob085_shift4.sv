// domain SysDomain

module TopModule (
  input logic clk,
  input logic areset,
  input logic load,
  input logic ena,
  input logic [4-1:0] data,
  output logic [4-1:0] q
);

  logic [4-1:0] q_r;
  always_ff @(posedge clk or posedge areset) begin
    if (areset) begin
      q_r <= 0;
    end else begin
      if (load) begin
        q_r <= data;
      end else if (ena) begin
        q_r[3] <= 0;
        q_r[2] <= q_r[3];
        q_r[1] <= q_r[2];
        q_r[0] <= q_r[1];
      end
    end
  end
  assign q = q_r;

endmodule

