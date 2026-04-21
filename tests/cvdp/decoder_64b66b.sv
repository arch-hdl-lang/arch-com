module decoder_64b66b (
  input logic clk_in,
  input logic rst_in,
  input logic decoder_data_valid_in,
  input logic [65:0] decoder_data_in,
  output logic [63:0] decoder_data_out,
  output logic [7:0] decoder_control_out,
  output logic sync_error,
  output logic decoder_error_out
);

  logic [1:0] sync_header;
  assign sync_header = decoder_data_in[65:64];
  logic [63:0] word64;
  assign word64 = decoder_data_in[63:0];
  always_ff @(posedge clk_in or posedge rst_in) begin
    if (rst_in) begin
      decoder_control_out <= 0;
      decoder_data_out <= 0;
      decoder_error_out <= 1'b0;
      sync_error <= 1'b0;
    end else begin
      if (~decoder_data_valid_in) begin
        decoder_data_out <= 0;
        decoder_control_out <= 0;
        sync_error <= 1'b0;
        decoder_error_out <= 1'b0;
      end else if (sync_header == 2'd1) begin
        decoder_data_out <= word64;
        decoder_control_out <= 0;
        sync_error <= 1'b0;
        decoder_error_out <= 1'b0;
      end else if (sync_header != 2'd2) begin
        decoder_data_out <= 0;
        decoder_control_out <= 0;
        sync_error <= 1'b1;
        decoder_error_out <= 1'b0;
      end else begin
        sync_error <= 1'b0;
        decoder_error_out <= 1'b0;
        decoder_data_out <= 0;
        decoder_control_out <= 0;
        if (word64 == 64'd2178749300044435230) begin
          decoder_data_out <= 64'd18374403900871474942;
          decoder_control_out <= 8'd255;
        end else if (word64 == 64'd2161727821137838080) begin
          decoder_data_out <= 64'd506381209866536711;
          decoder_control_out <= 8'd255;
        end else if (word64 == 64'd3737368369318330368) begin
          decoder_data_out <= 64'd15982355864460134151;
          decoder_control_out <= 8'd31;
        end else if (word64 == 64'd8661643059332439792) begin
          decoder_data_out <= 64'd3771334343958393083;
          decoder_control_out <= 8'd1;
        end else if (word64 == 64'd9727775195120271360) begin
          decoder_data_out <= 64'd506381209866536957;
          decoder_control_out <= 8'd254;
        end else if (word64 == 64'd11024811887802974382) begin
          decoder_data_out <= 64'd506381209866599854;
          decoder_control_out <= 8'd254;
        end else if (word64 == 64'd12249790986447791525) begin
          decoder_data_out <= 64'd506381209882699173;
          decoder_control_out <= 8'd252;
        end else if (word64 == 64'd12970366926843735381) begin
          decoder_data_out <= 64'd506381214009978197;
          decoder_control_out <= 8'd248;
        end else if (word64 == 64'd14699749186313156454) begin
          decoder_data_out <= 64'd506382268886447974;
          decoder_control_out <= 8'd240;
        end else if (word64 == 64'd15132094826152360080) begin
          decoder_data_out <= 64'd506651737731790992;
          decoder_control_out <= 8'd224;
        end else if (word64 == 64'd16213240059922267050) begin
          decoder_data_out <= 64'd575897728761772970;
          decoder_control_out <= 8'd192;
        end else if (word64 == 64'd18408238661689217450) begin
          decoder_data_out <= 64'd18264123473613361578;
          decoder_control_out <= 8'd128;
        end else if (word64 == 64'd6126873548985665287) begin
          decoder_data_out <= 64'd506381849816663964;
          decoder_control_out <= 8'd17;
        end else if (word64 == 64'd7383501467348299230) begin
          decoder_data_out <= 64'd8608481136402620060;
          decoder_control_out <= 8'd17;
        end else if (word64 == 64'd5404319553024745215) begin
          decoder_data_out <= 64'd506381211189835676;
          decoder_control_out <= 8'd241;
        end else if (word64 == 64'd3290630128895262720) begin
          decoder_data_out <= 64'd12297829319598081799;
          decoder_control_out <= 8'd31;
        end else begin
          decoder_error_out <= 1'b1;
        end
      end
    end
  end

endmodule

