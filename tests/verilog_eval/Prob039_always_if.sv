module TopModule (
  input logic a,
  input logic b,
  input logic sel_b1,
  input logic sel_b2,
  output logic out_assign,
  output logic out_always
);

  always_comb begin
    if ((sel_b1 & sel_b2)) begin
      out_assign = b;
      out_always = b;
    end else begin
      out_assign = a;
      out_always = a;
    end
  end

endmodule

