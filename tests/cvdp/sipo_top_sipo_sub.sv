module serial_in_parallel_out_8bit #(
  parameter int WIDTH = 64,
  parameter int SHIFT_DIRECTION = 1
) (
  input logic clk,
  input logic rst,
  input logic sin,
  input logic shift_en,
  output logic done,
  output logic [WIDTH-1:0] parallel_out
);

  logic [WIDTH-1:0] shift_reg;
  logic [$clog2(WIDTH + 1)-1:0] cnt;
  logic done_r;
  always_ff @(posedge clk) begin
    if ((!rst)) begin
      cnt <= 0;
      done_r <= 0;
      shift_reg <= 0;
    end else begin
      if (shift_en) begin
        if (SHIFT_DIRECTION == 1) begin
          shift_reg <= {shift_reg[WIDTH - 2:0], sin};
        end else begin
          shift_reg <= {sin, shift_reg[WIDTH - 1:1]};
        end
        if (cnt == WIDTH - 1) begin
          cnt <= 0;
          done_r <= 1;
        end else begin
          cnt <= ($clog2(WIDTH + 1))'(cnt + 1);
          done_r <= 0;
        end
      end else begin
        done_r <= 0;
      end
    end
  end
  assign parallel_out = shift_reg;
  assign done = done_r;

endmodule

