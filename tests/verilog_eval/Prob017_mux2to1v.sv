module TopModule (
  input logic [100-1:0] a,
  input logic [100-1:0] b,
  input logic sel,
  output logic [100-1:0] out
);

  always_comb begin
    if (sel) begin
      out = b;
    end else begin
      out = a;
    end
  end

endmodule

