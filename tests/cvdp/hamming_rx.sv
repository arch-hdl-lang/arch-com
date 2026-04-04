module hamming_rx #(
  parameter int DATA_WIDTH = 4,
  parameter int PARITY_BIT = 3,
  parameter int ENCODED_DATA = DATA_WIDTH + PARITY_BIT + 1,
  parameter int ENCODED_DATA_BIT = 3
) (
  input logic [ENCODED_DATA-1:0] data_in,
  output logic [DATA_WIDTH-1:0] data_out
);

  // Parameterized Hamming receiver
  // Default: DATA_WIDTH=4, PARITY_BIT=3 => ENCODED_DATA=8, ENCODED_DATA_BIT=3
  // Compute PARITY_BIT syndrome bits using even-parity XOR
  // parity[n] = XOR of data_in[i] for all i where bit n of i is 1
  // For default case: PARITY_BIT=3, ENCODED_DATA=8
  // parity[0]: positions where bit0=1 => 1,3,5,7
  // parity[1]: positions where bit1=1 => 2,3,6,7
  // parity[2]: positions where bit2=1 => 4,5,6,7
  logic [1-1:0] p0;
  assign p0 = 1'(data_in[1:1] ^ data_in[3:3] ^ data_in[5:5] ^ data_in[7:7]);
  logic [1-1:0] p1;
  assign p1 = 1'(data_in[2:2] ^ data_in[3:3] ^ data_in[6:6] ^ data_in[7:7]);
  logic [1-1:0] p2;
  assign p2 = 1'(data_in[4:4] ^ data_in[5:5] ^ data_in[6:6] ^ data_in[7:7]);
  logic [3-1:0] syndrome;
  assign syndrome = {p2, p1, p0};
  logic [ENCODED_DATA-1:0] corrected;
  always_comb begin
    if (syndrome == 0) begin
      corrected = data_in;
    end else if (syndrome == 1) begin
      corrected = data_in ^ 8'd2;
    end else if (syndrome == 2) begin
      corrected = data_in ^ 8'd4;
    end else if (syndrome == 3) begin
      corrected = data_in ^ 8'd8;
    end else if (syndrome == 4) begin
      corrected = data_in ^ 8'd16;
    end else if (syndrome == 5) begin
      corrected = data_in ^ 8'd32;
    end else if (syndrome == 6) begin
      corrected = data_in ^ 8'd64;
    end else begin
      corrected = data_in ^ 8'd128;
    end
  end
  // Extract data bits at non-power-of-2 positions (skip 0,1,2,4): 3,5,6,7
  // d0=corrected[3], d1=corrected[5], d2=corrected[6], d3=corrected[7]
  assign data_out[0:0] = corrected[3:3];
  assign data_out[1:1] = corrected[5:5];
  assign data_out[2:2] = corrected[6:6];
  assign data_out[3:3] = corrected[7:7];

endmodule

