module TopModule (
  input logic [8-1:0] in,
  output logic [3-1:0] pos
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
    end else if (in[4]) begin
      pos = 4;
    end else if (in[5]) begin
      pos = 5;
    end else if (in[6]) begin
      pos = 6;
    end else if (in[7]) begin
      pos = 7;
    end else begin
      pos = 0;
    end
  end

endmodule

