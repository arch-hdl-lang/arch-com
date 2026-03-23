module TopModule (
  input logic [8-1:0] a,
  input logic [8-1:0] b,
  input logic [8-1:0] c,
  input logic [8-1:0] d,
  output logic [8-1:0] min
);

  logic [8-1:0] ab_min;
  logic [8-1:0] cd_min;
  always_comb begin
    if ((a < b)) begin
      ab_min = a;
    end else begin
      ab_min = b;
    end
    if ((c < d)) begin
      cd_min = c;
    end else begin
      cd_min = d;
    end
    if ((ab_min < cd_min)) begin
      min = ab_min;
    end else begin
      min = cd_min;
    end
  end

endmodule

