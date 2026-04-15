module hill_cipher (
  input logic clk,
  input logic reset,
  input logic start,
  input logic [14:0] plaintext,
  input logic [44:0] key,
  output logic [14:0] ciphertext,
  output logic done
);

  logic [1:0] cyc;
  logic [9:0] p0;
  assign p0 = 10'($unsigned(plaintext[14:10]));
  logic [9:0] p1;
  assign p1 = 10'($unsigned(plaintext[9:5]));
  logic [9:0] p2;
  assign p2 = 10'($unsigned(plaintext[4:0]));
  logic [9:0] k00;
  assign k00 = 10'($unsigned(key[44:40]));
  logic [9:0] k01;
  assign k01 = 10'($unsigned(key[39:35]));
  logic [9:0] k02;
  assign k02 = 10'($unsigned(key[34:30]));
  logic [9:0] k10;
  assign k10 = 10'($unsigned(key[29:25]));
  logic [9:0] k11;
  assign k11 = 10'($unsigned(key[24:20]));
  logic [9:0] k12;
  assign k12 = 10'($unsigned(key[19:15]));
  logic [9:0] k20;
  assign k20 = 10'($unsigned(key[14:10]));
  logic [9:0] k21;
  assign k21 = 10'($unsigned(key[9:5]));
  logic [9:0] k22;
  assign k22 = 10'($unsigned(key[4:0]));
  logic [9:0] prod00;
  assign prod00 = 10'(k00 * p0);
  logic [9:0] prod01;
  assign prod01 = 10'(k01 * p1);
  logic [9:0] prod02;
  assign prod02 = 10'(k02 * p2);
  logic [9:0] prod10;
  assign prod10 = 10'(k10 * p0);
  logic [9:0] prod11;
  assign prod11 = 10'(k11 * p1);
  logic [9:0] prod12;
  assign prod12 = 10'(k12 * p2);
  logic [9:0] prod20;
  assign prod20 = 10'(k20 * p0);
  logic [9:0] prod21;
  assign prod21 = 10'(k21 * p1);
  logic [9:0] prod22;
  assign prod22 = 10'(k22 * p2);
  logic [4:0] m00;
  assign m00 = 5'(prod00 % 26);
  logic [4:0] m01;
  assign m01 = 5'(prod01 % 26);
  logic [4:0] m02;
  assign m02 = 5'(prod02 % 26);
  logic [4:0] m10;
  assign m10 = 5'(prod10 % 26);
  logic [4:0] m11;
  assign m11 = 5'(prod11 % 26);
  logic [4:0] m12;
  assign m12 = 5'(prod12 % 26);
  logic [4:0] m20;
  assign m20 = 5'(prod20 % 26);
  logic [4:0] m21;
  assign m21 = 5'(prod21 % 26);
  logic [4:0] m22;
  assign m22 = 5'(prod22 % 26);
  logic [7:0] sum0_ab;
  assign sum0_ab = 7'($unsigned(m00)) + 7'($unsigned(m01));
  logic [7:0] sum0;
  assign sum0 = 8'(sum0_ab + 8'($unsigned(m02)));
  logic [7:0] sum1_ab;
  assign sum1_ab = 7'($unsigned(m10)) + 7'($unsigned(m11));
  logic [7:0] sum1;
  assign sum1 = 8'(sum1_ab + 8'($unsigned(m12)));
  logic [7:0] sum2_ab;
  assign sum2_ab = 7'($unsigned(m20)) + 7'($unsigned(m21));
  logic [7:0] sum2;
  assign sum2 = 8'(sum2_ab + 8'($unsigned(m22)));
  logic [5:0] s0_mod64;
  assign s0_mod64 = 6'(sum0);
  logic [5:0] s1_mod64;
  assign s1_mod64 = 6'(sum1);
  logic [5:0] s2_mod64;
  assign s2_mod64 = 6'(sum2);
  logic [4:0] c0;
  assign c0 = 5'(s0_mod64 % 26);
  logic [4:0] c1;
  assign c1 = 5'(s1_mod64 % 26);
  logic [4:0] c2;
  assign c2 = 5'(s2_mod64 % 26);
  logic [14:0] result;
  assign result = {c0, c1, c2};
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      ciphertext <= 0;
      cyc <= 0;
      done <= 0;
    end else begin
      if (cyc == 0) begin
        done <= 0;
        if (start) begin
          cyc <= 1;
        end
      end else if (cyc == 1) begin
        cyc <= 2;
      end else if (cyc == 2) begin
        ciphertext <= result;
        done <= 1;
        cyc <= 0;
      end
    end
  end

endmodule

