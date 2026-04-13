module test_vec_temp #(
  parameter int IN_WIDTH = 3,
  parameter int T = 1,
  parameter int NG = 4
) (
  input logic [NG * IN_WIDTH-1:0] x,
  output logic [0:0] y
);

  logic signed [NG-1:0] [IN_WIDTH + 1-1:0] a;
  logic signed [NG-1:0] [IN_WIDTH + 2-1:0] b;
  logic [NG-1:0] [0:0] result;
  always_comb begin
    for (int i = 0; i <= NG - 1; i++) begin
      a[i] = 0;
      b[i] = 0;
    end
    for (int i = 0; i <= NG - 1; i++) begin
      if (a[i] - b[i] > 2 * T) begin
        result[i] = 1;
      end else if (a[i] - b[i] < -(2 * T)) begin
        result[i] = 1;
      end else begin
        result[i] = 0;
      end
    end
    y = result[0];
  end

endmodule

