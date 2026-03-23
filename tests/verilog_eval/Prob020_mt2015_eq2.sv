module TopModule (
  input logic [2-1:0] a_sig,
  input logic [2-1:0] b_sig,
  output logic z
);

  always_comb begin
    if ((a_sig == b_sig)) begin
      z = 1;
    end else begin
      z = 0;
    end
  end

endmodule

