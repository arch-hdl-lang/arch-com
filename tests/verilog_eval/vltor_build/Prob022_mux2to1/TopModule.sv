// domain SysDomain

module TopModule (
  input logic a,
  input logic b,
  input logic sel,
  output logic out
);

  always_comb begin
    if (sel == 1'd1) begin
      out = b;
    end else begin
      out = a;
    end
  end

endmodule

