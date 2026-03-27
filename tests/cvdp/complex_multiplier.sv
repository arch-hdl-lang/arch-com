module complex_multiplier (
  input logic clk,
  input logic arst_n,
  input logic signed [16-1:0] a_real,
  input logic signed [16-1:0] a_imag,
  input logic signed [16-1:0] b_real,
  input logic signed [16-1:0] b_imag,
  output logic signed [32-1:0] result_real,
  output logic signed [32-1:0] result_imag
);

  // (a+bj)*(c+dj) = (ac-bd) + (ad+bc)j
  logic signed [32-1:0] ac;
  logic signed [32-1:0] bd;
  logic signed [32-1:0] ad;
  logic signed [32-1:0] bc;
  assign ac = a_real * b_real;
  assign bd = a_imag * b_imag;
  assign ad = a_real * b_imag;
  assign bc = a_imag * b_real;
  always_ff @(posedge clk or negedge arst_n) begin
    if ((!arst_n)) begin
      result_imag <= 0;
      result_real <= 0;
    end else begin
      result_real <= 32'(ac - bd);
      result_imag <= 32'(ad + bc);
    end
  end

endmodule

