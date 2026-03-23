module TopModule (
  input logic clk,
  input logic x,
  input logic [3-1:0] y,
  output logic y0_sig,
  output logic z
);

  logic [4-1:0] yx;
  always_comb begin
    yx = 4'(((4'($unsigned(y)) * 2) + 4'($unsigned(x))));
    if ((yx == 0)) begin
      y0_sig = 0;
    end else if ((yx == 1)) begin
      y0_sig = 1;
    end else if ((yx == 2)) begin
      y0_sig = 1;
    end else if ((yx == 3)) begin
      y0_sig = 0;
    end else if ((yx == 4)) begin
      y0_sig = 0;
    end else if ((yx == 5)) begin
      y0_sig = 1;
    end else if ((yx == 6)) begin
      y0_sig = 1;
    end else if ((yx == 7)) begin
      y0_sig = 0;
    end else if ((yx == 8)) begin
      y0_sig = 1;
    end else begin
      y0_sig = 0;
    end
    if ((y == 3)) begin
      z = 1;
    end else if ((y == 4)) begin
      z = 1;
    end else begin
      z = 0;
    end
  end

endmodule

