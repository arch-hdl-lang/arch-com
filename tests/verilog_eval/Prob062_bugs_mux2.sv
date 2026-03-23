module TopModule (
  input logic sel,
  input logic [8-1:0] a,
  input logic [8-1:0] b,
  output logic [8-1:0] out_sig
);

  always_comb begin
    if (sel) begin
      out_sig = a;
    end else begin
      out_sig = b;
    end
  end

endmodule

