// domain SysDomain

module TopModule (
  input logic clk,
  input logic reset,
  output logic [32-1:0] q
);

  logic [32-1:0] q_r;
  always_ff @(posedge clk) begin
    if (reset) begin
      q_r <= 1;
    end else begin
      q_r[31] <= q_r[0];
      q_r[30] <= q_r[31];
      q_r[29] <= q_r[30];
      q_r[28] <= q_r[29];
      q_r[27] <= q_r[28];
      q_r[26] <= q_r[27];
      q_r[25] <= q_r[26];
      q_r[24] <= q_r[25];
      q_r[23] <= q_r[24];
      q_r[22] <= q_r[23];
      q_r[21] <= (q_r[22] ^ q_r[0]);
      q_r[20] <= q_r[21];
      q_r[19] <= q_r[20];
      q_r[18] <= q_r[19];
      q_r[17] <= q_r[18];
      q_r[16] <= q_r[17];
      q_r[15] <= q_r[16];
      q_r[14] <= q_r[15];
      q_r[13] <= q_r[14];
      q_r[12] <= q_r[13];
      q_r[11] <= q_r[12];
      q_r[10] <= q_r[11];
      q_r[9] <= q_r[10];
      q_r[8] <= q_r[9];
      q_r[7] <= q_r[8];
      q_r[6] <= q_r[7];
      q_r[5] <= q_r[6];
      q_r[4] <= q_r[5];
      q_r[3] <= q_r[4];
      q_r[2] <= q_r[3];
      q_r[1] <= (q_r[2] ^ q_r[0]);
      q_r[0] <= (q_r[1] ^ q_r[0]);
    end
  end
  assign q = q_r;

endmodule

