module TopModule (
  input logic [100-1:0] a,
  input logic [100-1:0] b,
  input logic sel,
  output logic [100-1:0] out_sig
);

  always_comb begin
    if (sel) begin
      out_sig = b;
    end else begin
      out_sig = a;
    end
  end

endmodule

