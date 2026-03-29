module compression_engine (
  input logic clk,
  input logic reset,
  input logic [24-1:0] num_i,
  output logic [12-1:0] mantissa_o,
  output logic [4-1:0] exponent_o
);

  // One-hot encoding of MSB position in bits [23:12]
  logic [12-1:0] exp_oh;
  assign exp_oh[11] = num_i[23];
  assign exp_oh[10] = num_i[22] & ~num_i[23];
  assign exp_oh[9] = num_i[21] & ~(num_i[23] | num_i[22]);
  assign exp_oh[8] = num_i[20] & ~(num_i[23] | num_i[22] | num_i[21]);
  assign exp_oh[7] = num_i[19] & ~(num_i[23] | num_i[22] | num_i[21] | num_i[20]);
  assign exp_oh[6] = num_i[18] & ~(num_i[23] | num_i[22] | num_i[21] | num_i[20] | num_i[19]);
  assign exp_oh[5] = num_i[17] & ~(num_i[23] | num_i[22] | num_i[21] | num_i[20] | num_i[19] | num_i[18]);
  assign exp_oh[4] = num_i[16] & ~(num_i[23] | num_i[22] | num_i[21] | num_i[20] | num_i[19] | num_i[18] | num_i[17]);
  assign exp_oh[3] = num_i[15] & ~(num_i[23] | num_i[22] | num_i[21] | num_i[20] | num_i[19] | num_i[18] | num_i[17] | num_i[16]);
  assign exp_oh[2] = num_i[14] & ~(num_i[23] | num_i[22] | num_i[21] | num_i[20] | num_i[19] | num_i[18] | num_i[17] | num_i[16] | num_i[15]);
  assign exp_oh[1] = num_i[13] & ~(num_i[23] | num_i[22] | num_i[21] | num_i[20] | num_i[19] | num_i[18] | num_i[17] | num_i[16] | num_i[15] | num_i[14]);
  assign exp_oh[0] = num_i[12] & ~(num_i[23] | num_i[22] | num_i[21] | num_i[20] | num_i[19] | num_i[18] | num_i[17] | num_i[16] | num_i[15] | num_i[14] | num_i[13]);
  // One-hot to binary conversion
  logic [4-1:0] exp_bin;
  always_comb begin
    exp_bin = 0;
    for (int i = 0; i <= 11; i++) begin
      if (exp_oh[i +: 1]) begin
        exp_bin = 4'(i);
      end
    end
  end
  // Adjusted exponent: exp_bin+1 if any bit set, else 0
  logic any_oh;
  assign any_oh = exp_oh != 0;
  logic [4-1:0] exponent_w;
  always_comb begin
    if (any_oh) begin
      exponent_w = 4'(exp_bin + 1);
    end else begin
      exponent_w = 0;
    end
  end
  // Mantissa extraction: shift right by (exponent-1) when exponent>=1
  // exponent=0: num_i[11:0]
  // exponent=1: num_i[11:0]  (shift by 0)
  // exponent=2: num_i[12:1]
  // exponent=N (N>=1): num_i[N+10:N-1]
  logic [12-1:0] mantissa_w;
  always_comb begin
    if (exponent_w == 0) begin
      mantissa_w = num_i[11:0];
    end else if (exponent_w == 1) begin
      mantissa_w = num_i[11:0];
    end else if (exponent_w == 2) begin
      mantissa_w = num_i[12:1];
    end else if (exponent_w == 3) begin
      mantissa_w = num_i[13:2];
    end else if (exponent_w == 4) begin
      mantissa_w = num_i[14:3];
    end else if (exponent_w == 5) begin
      mantissa_w = num_i[15:4];
    end else if (exponent_w == 6) begin
      mantissa_w = num_i[16:5];
    end else if (exponent_w == 7) begin
      mantissa_w = num_i[17:6];
    end else if (exponent_w == 8) begin
      mantissa_w = num_i[18:7];
    end else if (exponent_w == 9) begin
      mantissa_w = num_i[19:8];
    end else if (exponent_w == 10) begin
      mantissa_w = num_i[20:9];
    end else if (exponent_w == 11) begin
      mantissa_w = num_i[21:10];
    end else begin
      mantissa_w = num_i[22:11];
    end
  end
  // Registered outputs
  logic [12-1:0] mantissa_r;
  logic [4-1:0] exponent_r;
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      exponent_r <= 0;
      mantissa_r <= 0;
    end else begin
      mantissa_r <= mantissa_w;
      exponent_r <= exponent_w;
    end
  end
  assign mantissa_o = mantissa_r;
  assign exponent_o = exponent_r;

endmodule

