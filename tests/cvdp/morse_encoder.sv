module morse_encoder (
  input logic [8-1:0] ascii_in,
  output logic [10-1:0] morse_out,
  output logic [4-1:0] morse_length
);

  always_comb begin
    if (ascii_in == 8'd65) begin
      morse_out = 10'd1;
      morse_length = 4'd2;
    end else if (ascii_in == 8'd66) begin
      morse_out = 10'd8;
      morse_length = 4'd4;
    end else if (ascii_in == 8'd67) begin
      morse_out = 10'd10;
      morse_length = 4'd4;
    end else if (ascii_in == 8'd68) begin
      morse_out = 10'd4;
      morse_length = 4'd3;
    end else if (ascii_in == 8'd69) begin
      morse_out = 10'd0;
      morse_length = 4'd1;
    end else if (ascii_in == 8'd70) begin
      morse_out = 10'd2;
      morse_length = 4'd4;
    end else if (ascii_in == 8'd71) begin
      morse_out = 10'd6;
      morse_length = 4'd3;
    end else if (ascii_in == 8'd72) begin
      morse_out = 10'd0;
      morse_length = 4'd4;
    end else if (ascii_in == 8'd73) begin
      morse_out = 10'd0;
      morse_length = 4'd2;
    end else if (ascii_in == 8'd74) begin
      morse_out = 10'd7;
      morse_length = 4'd4;
    end else if (ascii_in == 8'd75) begin
      morse_out = 10'd5;
      morse_length = 4'd3;
    end else if (ascii_in == 8'd76) begin
      morse_out = 10'd4;
      morse_length = 4'd4;
    end else if (ascii_in == 8'd77) begin
      morse_out = 10'd3;
      morse_length = 4'd2;
    end else if (ascii_in == 8'd78) begin
      morse_out = 10'd2;
      morse_length = 4'd2;
    end else if (ascii_in == 8'd79) begin
      morse_out = 10'd7;
      morse_length = 4'd3;
    end else if (ascii_in == 8'd80) begin
      morse_out = 10'd6;
      morse_length = 4'd4;
    end else if (ascii_in == 8'd81) begin
      morse_out = 10'd13;
      morse_length = 4'd4;
    end else if (ascii_in == 8'd82) begin
      morse_out = 10'd2;
      morse_length = 4'd3;
    end else if (ascii_in == 8'd83) begin
      morse_out = 10'd0;
      morse_length = 4'd3;
    end else if (ascii_in == 8'd84) begin
      morse_out = 10'd1;
      morse_length = 4'd1;
    end else if (ascii_in == 8'd85) begin
      morse_out = 10'd1;
      morse_length = 4'd3;
    end else if (ascii_in == 8'd86) begin
      morse_out = 10'd1;
      morse_length = 4'd4;
    end else if (ascii_in == 8'd87) begin
      morse_out = 10'd3;
      morse_length = 4'd3;
    end else if (ascii_in == 8'd88) begin
      morse_out = 10'd9;
      morse_length = 4'd4;
    end else if (ascii_in == 8'd89) begin
      morse_out = 10'd11;
      morse_length = 4'd4;
    end else if (ascii_in == 8'd90) begin
      morse_out = 10'd12;
      morse_length = 4'd4;
    end else if (ascii_in == 8'd48) begin
      morse_out = 10'd31;
      morse_length = 4'd5;
    end else if (ascii_in == 8'd49) begin
      morse_out = 10'd15;
      morse_length = 4'd5;
    end else if (ascii_in == 8'd50) begin
      morse_out = 10'd7;
      morse_length = 4'd5;
    end else if (ascii_in == 8'd51) begin
      morse_out = 10'd3;
      morse_length = 4'd5;
    end else if (ascii_in == 8'd52) begin
      morse_out = 10'd1;
      morse_length = 4'd5;
    end else if (ascii_in == 8'd53) begin
      morse_out = 10'd0;
      morse_length = 4'd5;
    end else if (ascii_in == 8'd54) begin
      morse_out = 10'd16;
      morse_length = 4'd5;
    end else if (ascii_in == 8'd55) begin
      morse_out = 10'd24;
      morse_length = 4'd5;
    end else if (ascii_in == 8'd56) begin
      morse_out = 10'd28;
      morse_length = 4'd5;
    end else if (ascii_in == 8'd57) begin
      morse_out = 10'd30;
      morse_length = 4'd5;
    end else begin
      morse_out = 10'd0;
      morse_length = 4'd0;
    end
  end

endmodule

