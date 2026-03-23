module TopModule (
  input logic [4-1:0] in_sig,
  output logic [2-1:0] pos
);

  always_comb begin
    if (in_sig[0]) begin
      pos = 0;
    end else if (in_sig[1]) begin
      pos = 1;
    end else if (in_sig[2]) begin
      pos = 2;
    end else if (in_sig[3]) begin
      pos = 3;
    end else begin
      pos = 0;
    end
  end

endmodule

