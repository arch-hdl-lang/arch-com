module TopModule (
  input logic do_sub,
  input logic [8-1:0] a,
  input logic [8-1:0] b,
  output logic [8-1:0] out_sig,
  output logic result_is_zero
);

  always_comb begin
    if (do_sub) begin
      out_sig = 8'((a - b));
    end else begin
      out_sig = 8'((a + b));
    end
    if ((out_sig == 0)) begin
      result_is_zero = 1;
    end else begin
      result_is_zero = 0;
    end
  end

endmodule

