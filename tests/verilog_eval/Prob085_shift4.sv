module TopModule (
  input logic clk,
  input logic areset,
  input logic load,
  input logic ena,
  input logic [4-1:0] data,
  output logic [4-1:0] q
);

  always_ff @(posedge clk or posedge areset) begin
    if (areset) begin
      q <= 0;
    end else begin
      if (load) begin
        q <= data;
      end else if (ena) begin
        q[3] <= 0;
        q[2] <= q[3];
        q[1] <= q[2];
        q[0] <= q[1];
      end
    end
  end

endmodule

