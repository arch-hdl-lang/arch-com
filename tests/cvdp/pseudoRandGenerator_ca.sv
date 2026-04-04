module pseudoRandGenerator_ca (
  input logic clock,
  input logic reset,
  input logic [16-1:0] CA_seed,
  output logic [16-1:0] CA_out
);

  // Rule pattern: R90-R90-R150-R90-R150-R90-R150-R90-R150-R90-R150-R90-R150-R90-R150-R90
  // Bit 15: R90, Bit 14: R90, Bit 13: R150, Bit 12: R90, ...
  // R90:  next[i] = left ^ right
  // R150: next[i] = left ^ self ^ right
  // Boundary: neighbors outside 0..15 treated as 0
  logic [16-1:0] next_ca;
  assign next_ca[15:15] = CA_out[14:14];
  assign next_ca[14:14] = CA_out[15:15] ^ CA_out[13:13];
  assign next_ca[13:13] = CA_out[14:14] ^ CA_out[13:13] ^ CA_out[12:12];
  assign next_ca[12:12] = CA_out[13:13] ^ CA_out[11:11];
  assign next_ca[11:11] = CA_out[12:12] ^ CA_out[11:11] ^ CA_out[10:10];
  assign next_ca[10:10] = CA_out[11:11] ^ CA_out[9:9];
  assign next_ca[9:9] = CA_out[10:10] ^ CA_out[9:9] ^ CA_out[8:8];
  assign next_ca[8:8] = CA_out[9:9] ^ CA_out[7:7];
  assign next_ca[7:7] = CA_out[8:8] ^ CA_out[7:7] ^ CA_out[6:6];
  assign next_ca[6:6] = CA_out[7:7] ^ CA_out[5:5];
  assign next_ca[5:5] = CA_out[6:6] ^ CA_out[5:5] ^ CA_out[4:4];
  assign next_ca[4:4] = CA_out[5:5] ^ CA_out[3:3];
  assign next_ca[3:3] = CA_out[4:4] ^ CA_out[3:3] ^ CA_out[2:2];
  assign next_ca[2:2] = CA_out[3:3] ^ CA_out[1:1];
  assign next_ca[1:1] = CA_out[2:2] ^ CA_out[1:1] ^ CA_out[0:0];
  assign next_ca[0:0] = CA_out[1:1];
  // Bit 15 (R90): left=0, right=CA_out[14]
  // Bit 14 (R90): left=CA_out[15], right=CA_out[13]
  // Bit 13 (R150): left=CA_out[14], self=CA_out[13], right=CA_out[12]
  // Bit 12 (R90)
  // Bit 11 (R150)
  // Bit 10 (R90)
  // Bit 9 (R150)
  // Bit 8 (R90)
  // Bit 7 (R150)
  // Bit 6 (R90)
  // Bit 5 (R150)
  // Bit 4 (R90)
  // Bit 3 (R150)
  // Bit 2 (R90)
  // Bit 1 (R150)
  // Bit 0 (R90): left=CA_out[1], right=0
  always_ff @(posedge clock) begin
    if (reset) begin
      CA_out <= 0;
    end else begin
      CA_out <= next_ca;
    end
  end

endmodule

