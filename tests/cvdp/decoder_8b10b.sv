module decoder_8b10b (
  input logic clk_in,
  input logic reset_in,
  input logic [9:0] decoder_in,
  output logic [7:0] decoder_out,
  output logic control_out
);

  logic [7:0] dec_val;
  logic ctrl_val;
  always_comb begin
    if (decoder_in == 10'd244 || decoder_in == 10'd779) begin
      dec_val = 8'd28;
      ctrl_val = 1'b1;
    end else if (decoder_in == 10'd249 || decoder_in == 10'd774) begin
      dec_val = 8'd60;
      ctrl_val = 1'b1;
    end else if (decoder_in == 10'd245 || decoder_in == 10'd778) begin
      dec_val = 8'd92;
      ctrl_val = 1'b1;
    end else if (decoder_in == 10'd243 || decoder_in == 10'd780) begin
      dec_val = 8'd124;
      ctrl_val = 1'b1;
    end else if (decoder_in == 10'd242 || decoder_in == 10'd781) begin
      dec_val = 8'd156;
      ctrl_val = 1'b1;
    end else if (decoder_in == 10'd250 || decoder_in == 10'd773) begin
      dec_val = 8'd188;
      ctrl_val = 1'b1;
    end else if (decoder_in == 10'd246 || decoder_in == 10'd777) begin
      dec_val = 8'd220;
      ctrl_val = 1'b1;
    end else if (decoder_in == 10'd248 || decoder_in == 10'd775) begin
      dec_val = 8'd252;
      ctrl_val = 1'b1;
    end else if (decoder_in == 10'd936 || decoder_in == 10'd87) begin
      dec_val = 8'd247;
      ctrl_val = 1'b1;
    end else if (decoder_in == 10'd872 || decoder_in == 10'd151) begin
      dec_val = 8'd251;
      ctrl_val = 1'b1;
    end else if (decoder_in == 10'd744 || decoder_in == 10'd279) begin
      dec_val = 8'd253;
      ctrl_val = 1'b1;
    end else if (decoder_in == 10'd488 || decoder_in == 10'd535) begin
      dec_val = 8'd254;
      ctrl_val = 1'b1;
    end else begin
      dec_val = 8'd0;
      ctrl_val = 1'b0;
    end
  end
  always_ff @(posedge clk_in or posedge reset_in) begin
    if (reset_in) begin
      control_out <= 0;
      decoder_out <= 0;
    end else begin
      decoder_out <= dec_val;
      control_out <= ctrl_val;
    end
  end

endmodule

