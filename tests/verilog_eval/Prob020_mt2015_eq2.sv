module TopModule (
  input logic [2-1:0] A,
  input logic [2-1:0] B,
  output logic z
);

  always_comb begin
    if ((A == B)) begin
      z = 1;
    end else begin
      z = 0;
    end
  end

endmodule

