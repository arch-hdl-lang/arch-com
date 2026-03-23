// domain SysDomain

module TopModule (
  input logic a,
  input logic b,
  input logic sel,
  output logic out_sig
);

  always_comb begin
    if ((sel == 1'd1)) begin
      out_sig = b;
    end else begin
      out_sig = a;
    end
  end

endmodule

