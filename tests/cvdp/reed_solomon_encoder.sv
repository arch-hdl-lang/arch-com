module reed_solomon_encoder #(
  parameter int DATA_WIDTH = 8,
  parameter int N = 255,
  parameter int K = 223
) (
  input logic clk,
  input logic reset,
  input logic enable,
  input logic [DATA_WIDTH-1:0] data_in,
  input logic valid_in,
  output logic [DATA_WIDTH-1:0] codeword_out,
  output logic valid_out,
  output logic [DATA_WIDTH-1:0] parity_0,
  output logic [DATA_WIDTH-1:0] parity_1
);

  // Generator polynomial coefficient
  logic [DATA_WIDTH-1:0] gp;
  assign gp = 8'd51;
  // Feedback: combinational
  logic [DATA_WIDTH-1:0] feedback;
  assign feedback = data_in ^ parity_1;
  // feedback * gp product truncated to DATA_WIDTH
  logic [16-1:0] fb_prod;
  assign fb_prod = feedback * gp;
  logic [DATA_WIDTH-1:0] fb_mult;
  assign fb_mult = fb_prod[DATA_WIDTH - 1:0];
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      codeword_out <= 0;
      parity_0 <= 0;
      parity_1 <= 0;
      valid_out <= 1'b0;
    end else begin
      if (reset) begin
        parity_0 <= 0;
        parity_1 <= 0;
        codeword_out <= 0;
        valid_out <= 1'b0;
      end else if (enable & valid_in) begin
        parity_0 <= feedback;
        parity_1 <= parity_0 ^ fb_mult;
        codeword_out <= data_in;
        valid_out <= 1'b1;
      end
    end
  end

endmodule

