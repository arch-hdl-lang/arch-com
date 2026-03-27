module TopModule (
  input logic sel,
  input logic [8-1:0] a,
  input logic [8-1:0] b,
  output logic [8-1:0] out
);

  always_comb begin
    if (sel) begin
      out = a;
    end else begin
      out = b;
    end
  end

endmodule

