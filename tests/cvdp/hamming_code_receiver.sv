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
    if (syndrome == 3'd0) begin
      corrected = data_in;
    end else if (syndrome == 3'd1) begin
      corrected = data_in ^ 8'd2;
    end else if (syndrome == 3'd2) begin
      corrected = data_in ^ 8'd4;
    end else if (syndrome == 3'd3) begin
      corrected = data_in ^ 8'd8;
    end else if (syndrome == 3'd4) begin
      corrected = data_in ^ 8'd16;
    end else if (syndrome == 3'd5) begin
      corrected = data_in ^ 8'd32;
    end else if (syndrome == 3'd6) begin
      corrected = data_in ^ 8'd64;
    end else begin
      corrected = data_in ^ 8'd128;
    end
    // d3=corrected[7], d2=corrected[6], d1=corrected[5], d0=corrected[3]
    data_out = {corrected[7], corrected[6], corrected[5], corrected[3]};
  end

endmodule

