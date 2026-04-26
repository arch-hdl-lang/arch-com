module hamming_code_receiver (
  input logic [7:0] data_in,
  output logic [3:0] data_out
);

  logic c1;
  logic c2;
  logic c3;
  logic [2:0] syndrome;
  logic [7:0] corrected;
  always_comb begin
    c1 = data_in[1] ^ data_in[3] ^ data_in[5] ^ data_in[7];
    c2 = data_in[2] ^ data_in[3] ^ data_in[6] ^ data_in[7];
    c3 = data_in[4] ^ data_in[5] ^ data_in[6] ^ data_in[7];
    syndrome = {c3, c2, c1};
    // Syndrome k (1..7) flips bit k of data_in; syndrome 0 = no error.
    case (syndrome)
      3'd0: corrected = data_in;
      default: corrected = 8'(data_in ^ 8'd1 << syndrome);
    endcase
    // d3=corrected[7], d2=corrected[6], d1=corrected[5], d0=corrected[3]
    data_out = {corrected[7], corrected[6], corrected[5], corrected[3]};
  end

endmodule

