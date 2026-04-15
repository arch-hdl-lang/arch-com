module crossbar_switch #(
  parameter int DATA_WIDTH = 8,
  parameter int NUM_PORTS = 4,
  parameter int DATA_WIDTH_IN = DATA_WIDTH + 2
) (
  input logic clk,
  input logic reset,
  input logic [DATA_WIDTH_IN-1:0] in0,
  input logic [DATA_WIDTH_IN-1:0] in1,
  input logic [DATA_WIDTH_IN-1:0] in2,
  input logic [DATA_WIDTH_IN-1:0] in3,
  input logic valid_in0,
  input logic valid_in1,
  input logic valid_in2,
  input logic valid_in3,
  output logic [DATA_WIDTH-1:0] out0,
  output logic [DATA_WIDTH-1:0] out1,
  output logic [DATA_WIDTH-1:0] out2,
  output logic [DATA_WIDTH-1:0] out3,
  output logic valid_out0,
  output logic valid_out1,
  output logic valid_out2,
  output logic valid_out3
);

  logic [1:0] dest0;
  assign dest0 = in0[DATA_WIDTH_IN - 2 +: 2];
  logic [1:0] dest1;
  assign dest1 = in1[DATA_WIDTH_IN - 2 +: 2];
  logic [1:0] dest2;
  assign dest2 = in2[DATA_WIDTH_IN - 2 +: 2];
  logic [1:0] dest3;
  assign dest3 = in3[DATA_WIDTH_IN - 2 +: 2];
  logic [DATA_WIDTH-1:0] data0;
  assign data0 = in0[DATA_WIDTH - 1:0];
  logic [DATA_WIDTH-1:0] data1;
  assign data1 = in1[DATA_WIDTH - 1:0];
  logic [DATA_WIDTH-1:0] data2;
  assign data2 = in2[DATA_WIDTH - 1:0];
  logic [DATA_WIDTH-1:0] data3;
  assign data3 = in3[DATA_WIDTH - 1:0];
  always_ff @(posedge clk or negedge reset) begin
    if ((!reset)) begin
      out0 <= 0;
      out1 <= 0;
      out2 <= 0;
      out3 <= 0;
      valid_out0 <= 0;
      valid_out1 <= 0;
      valid_out2 <= 0;
      valid_out3 <= 0;
    end else begin
      out0 <= 0;
      out1 <= 0;
      out2 <= 0;
      out3 <= 0;
      valid_out0 <= 0;
      valid_out1 <= 0;
      valid_out2 <= 0;
      valid_out3 <= 0;
      if (valid_in0 == 1) begin
        if (dest0 == 0) begin
          out0 <= data0;
          valid_out0 <= 1;
        end else if (dest0 == 1) begin
          out1 <= data0;
          valid_out1 <= 1;
        end else if (dest0 == 2) begin
          out2 <= data0;
          valid_out2 <= 1;
        end else begin
          out3 <= data0;
          valid_out3 <= 1;
        end
      end else if (valid_in1 == 1) begin
        if (dest1 == 0) begin
          out0 <= data1;
          valid_out0 <= 1;
        end else if (dest1 == 1) begin
          out1 <= data1;
          valid_out1 <= 1;
        end else if (dest1 == 2) begin
          out2 <= data1;
          valid_out2 <= 1;
        end else begin
          out3 <= data1;
          valid_out3 <= 1;
        end
      end else if (valid_in2 == 1) begin
        if (dest2 == 0) begin
          out0 <= data2;
          valid_out0 <= 1;
        end else if (dest2 == 1) begin
          out1 <= data2;
          valid_out1 <= 1;
        end else if (dest2 == 2) begin
          out2 <= data2;
          valid_out2 <= 1;
        end else begin
          out3 <= data2;
          valid_out3 <= 1;
        end
      end else if (valid_in3 == 1) begin
        if (dest3 == 0) begin
          out0 <= data3;
          valid_out0 <= 1;
        end else if (dest3 == 1) begin
          out1 <= data3;
          valid_out1 <= 1;
        end else if (dest3 == 2) begin
          out2 <= data3;
          valid_out2 <= 1;
        end else begin
          out3 <= data3;
          valid_out3 <= 1;
        end
      end
    end
  end

endmodule

