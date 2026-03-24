module TopModule (
  input logic [3-1:0] y,
  input logic w,
  output logic Y1
);

  always_comb begin
    if ((((y == 1) | (y == 5)) | (w & ((y == 2) | (y == 4))))) begin
      Y1 = 1;
    end else begin
      Y1 = 0;
    end
  end

endmodule

