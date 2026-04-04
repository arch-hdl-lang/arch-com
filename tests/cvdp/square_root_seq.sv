// Sequential square root using digit-by-digit algorithm
// Ports: clk, rst (sync high), start, num (WIDTH-bit), done, final_root (WIDTH/2-bit)
module square_root_seq #(
  parameter int WIDTH = 16
) (
  input logic clk,
  input logic rst,
  input logic start,
  input logic [WIDTH-1:0] num,
  output logic done,
  output logic [WIDTH-1:0] final_root
);

  logic running;
  logic finished;
  logic [WIDTH-1:0] radicand;
  logic [WIDTH-1:0] root_r;
  logic [WIDTH-1:0] remainder;
  logic [WIDTH-1:0] bit_r;
  logic [WIDTH-1:0] result_r;
  assign done = finished;
  assign final_root = result_r;
  always_ff @(posedge clk) begin
    if (rst) begin
      running <= 1'b0;
      finished <= 1'b0;
      radicand <= 0;
      root_r <= 0;
      remainder <= 0;
      bit_r <= 0;
      result_r <= 0;
    end else if (start & ~running & ~finished) begin
      running <= 1'b1;
      finished <= 1'b0;
      radicand <= num;
      root_r <= 0;
      remainder <= 0;
      // Start bit: highest even power of 4 not exceeding num (use 1<<(WIDTH-2) as initial)
      bit_r <= 1 << WIDTH - 2;
      result_r <= 0;
    end else if (running) begin
      if (bit_r == 0) begin
        running <= 1'b0;
        finished <= 1'b1;
        result_r <= root_r;
      end else begin
        if (radicand >= root_r + bit_r) begin
          radicand <= WIDTH'(radicand - root_r - bit_r);
          root_r <= WIDTH'((root_r >> 1) + bit_r);
        end else begin
          root_r <= root_r >> 1;
        end
        bit_r <= bit_r >> 2;
      end
    end else if (finished & ~start) begin
      finished <= 1'b0;
    end
  end

endmodule

