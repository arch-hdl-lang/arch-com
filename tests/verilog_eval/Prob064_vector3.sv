module TopModule (
  input logic [5-1:0] a,
  input logic [5-1:0] b,
  input logic [5-1:0] c,
  input logic [5-1:0] d,
  input logic [5-1:0] e,
  input logic [5-1:0] f,
  output logic [8-1:0] w,
  output logic [8-1:0] x,
  output logic [8-1:0] y,
  output logic [8-1:0] z
);

  logic [32-1:0] cat;
  always_comb begin
    cat[0] = 1;
    cat[1] = 1;
    for (int i = 0; i <= 4; i++) begin
      cat[(2 + i)] = f[i];
      cat[(7 + i)] = e[i];
      cat[(12 + i)] = d[i];
      cat[(17 + i)] = c[i];
      cat[(22 + i)] = b[i];
      cat[(27 + i)] = a[i];
    end
    for (int i = 0; i <= 7; i++) begin
      z[i] = cat[i];
      y[i] = cat[(8 + i)];
      x[i] = cat[(16 + i)];
      w[i] = cat[(24 + i)];
    end
  end

endmodule

