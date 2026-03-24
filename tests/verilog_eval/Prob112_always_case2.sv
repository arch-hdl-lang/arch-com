module TopModule (
  input logic [4-1:0] in,
  output logic [2-1:0] pos
);

  always_comb begin
    if (in[0]) begin
      pos = 0;
    end else if (in[1]) begin
      pos = 1;
    end else if (in[2]) begin
      pos = 2;
    end else if (in[3]) begin
      pos = 3;
    end else begin
      pos = 0;
    end
  end

endmodule

