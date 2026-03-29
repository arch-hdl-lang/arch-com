module FILO_RTL #(
  parameter int DATA_WIDTH = 8,
  parameter int FILO_DEPTH = 16
) (
  input logic clk,
  input logic reset,
  input logic push,
  input logic pop,
  input logic [DATA_WIDTH-1:0] data_in,
  output logic [DATA_WIDTH-1:0] data_out,
  output logic full,
  output logic empty
);

  logic [32-1:0] top;
  logic [DATA_WIDTH-1:0] buffer [0:FILO_DEPTH-1];
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      for (int __ri0 = 0; __ri0 < FILO_DEPTH; __ri0++) begin
        buffer[__ri0] <= 0;
      end
      data_out <= 0;
      empty <= 1;
      full <= 0;
      top <= 0;
    end else begin
      if (push & ~pop & ~full) begin
        buffer[top] <= data_in;
        top <= 32'(top + 1);
        empty <= 1'b0;
        if (32'(top + 1) == 32'(FILO_DEPTH)) begin
          full <= 1'b1;
        end
      end else if (pop & ~push & ~empty) begin
        top <= 32'(top - 1);
        data_out <= buffer[32'(top - 1)];
        full <= 1'b0;
        if (32'(top - 1) == 0) begin
          empty <= 1'b1;
        end
      end else if (push & pop & empty) begin
        data_out <= data_in;
      end
    end
  end

endmodule

