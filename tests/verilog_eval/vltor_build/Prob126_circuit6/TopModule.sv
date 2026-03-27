module TopModule (
  input logic [3-1:0] a,
  output logic [16-1:0] q
);

  always_comb begin
    if (a == 0) begin
      q = 4658;
    end else if (a == 1) begin
      q = 44768;
    end else if (a == 2) begin
      q = 10196;
    end else if (a == 3) begin
      q = 23054;
    end else if (a == 4) begin
      q = 8294;
    end else if (a == 5) begin
      q = 25806;
    end else if (a == 6) begin
      q = 50470;
    end else begin
      q = 12057;
    end
  end

endmodule

