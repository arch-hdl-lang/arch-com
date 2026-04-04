module binary_multiplier #(
  parameter int WIDTH = 32
) (
  input logic clk,
  input logic rst_n,
  input logic valid_in,
  input logic [WIDTH-1:0] A,
  input logic [WIDTH-1:0] B,
  output logic valid_out,
  output logic [2 * WIDTH-1:0] Product
);

  logic [WIDTH-1:0] a_reg;
  logic [WIDTH-1:0] b_reg;
  logic [2 * WIDTH-1:0] acc;
  logic [5-1:0] bit_idx;
  logic [2-1:0] phase;
  logic running;
  logic [2 * WIDTH-1:0] prod_reg;
  logic vout;
  assign valid_out = vout;
  assign Product = prod_reg;
  // phase 0: iterating through bits 0..31
  // phase 1: done, output result
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      a_reg <= 0;
      acc <= 0;
      b_reg <= 0;
      bit_idx <= 0;
      phase <= 0;
      prod_reg <= 0;
      running <= 1'b0;
      vout <= 1'b0;
    end else begin
      vout <= 1'b0;
      if (running) begin
        if (phase == 0) begin
          if (a_reg[bit_idx +: 1] == 1) begin
            acc <= (2 * WIDTH)'(acc + ((2 * WIDTH)'($unsigned(b_reg)) << bit_idx));
          end
          if (bit_idx == 31) begin
            phase <= 1;
            bit_idx <= 0;
          end else begin
            bit_idx <= 5'(bit_idx + 1);
          end
        end else begin
          prod_reg <= acc;
          vout <= 1'b1;
          running <= 1'b0;
          phase <= 0;
          acc <= 0;
        end
      end
      if (valid_in) begin
        a_reg <= A;
        b_reg <= B;
        acc <= 0;
        bit_idx <= 0;
        phase <= 0;
        running <= 1'b1;
      end
    end
  end

endmodule

