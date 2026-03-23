module TopModule (
  input logic [8-1:0] in_sig,
  output logic [3-1:0] pos
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
    end else if (in_sig[4]) begin
      pos = 4;
    end else if (in_sig[5]) begin
      pos = 5;
    end else if (in_sig[6]) begin
      pos = 6;
    end else if (in_sig[7]) begin
      pos = 7;
    end else begin
      pos = 0;
    end
  end

endmodule

