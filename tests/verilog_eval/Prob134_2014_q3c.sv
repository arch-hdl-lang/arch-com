Wrote tests/verilog_eval/Prob134_2014_q3c.sv
 logic x,
  input logic [3-1:0] y,
  output logic Y0,
  output logic z
);

  logic [4-1:0] yx;
  always_comb begin
    yx = 4'(4'($unsigned(y)) * 2 + 4'($unsigned(x)));
    if (yx == 0) begin
      Y0 = 0;
    end else if (yx == 1) begin
      Y0 = 1;
    end else if (yx == 2) begin
      Y0 = 1;
    end else if (yx == 3) begin
      Y0 = 0;
    end else if (yx == 4) begin
      Y0 = 0;
    end else if (yx == 5) begin
      Y0 = 1;
    end else if (yx == 6) begin
      Y0 = 1;
    end else if (yx == 7) begin
      Y0 = 0;
    end else if (yx == 8) begin
      Y0 = 1;
    end else begin
      Y0 = 0;
    end
    z = y == 3 | y == 4;
  end

endmodule

