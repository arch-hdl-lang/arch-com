module restoring_division #(
  parameter int WIDTH = 6
) (
  input logic clk,
  input logic rst,
  input logic start,
  input logic [WIDTH-1:0] dividend,
  input logic [WIDTH-1:0] divisor,
  output logic [WIDTH-1:0] quotient,
  output logic [WIDTH-1:0] remainder,
  output logic valid
);

  logic running;
  logic [WIDTH-1:0] count;
  logic [WIDTH-1:0] dvd;
  logic [WIDTH-1:0] dvs;
  logic [WIDTH-1:0] q_reg;
  logic [WIDTH + 1-1:0] rem_reg;
  logic [WIDTH + 1-1:0] shifted_rem;
  logic [WIDTH + 1-1:0] sub_result;
  logic [1-1:0] next_q_bit;
  always_comb begin
    shifted_rem = {rem_reg[WIDTH - 1:0], dvd[WIDTH - 1 +: 1]};
    sub_result = (WIDTH + 1)'(shifted_rem - (WIDTH + 1)'($unsigned(dvs)));
    if (~sub_result[WIDTH]) begin
      next_q_bit = 1;
    end else begin
      next_q_bit = 0;
    end
  end
  always_ff @(posedge clk or negedge rst) begin
    if ((!rst)) begin
      count <= 0;
      dvd <= 0;
      dvs <= 0;
      q_reg <= 0;
      quotient <= 0;
      rem_reg <= 0;
      remainder <= 0;
      running <= 1'b0;
      valid <= 1'b0;
    end else begin
      valid <= 1'b0;
      if (start & ~running) begin
        running <= 1'b1;
        count <= 0;
        dvd <= dividend;
        dvs <= divisor;
        q_reg <= 0;
        rem_reg <= 0;
      end else if (running) begin
        if (~sub_result[WIDTH]) begin
          rem_reg <= sub_result;
        end else begin
          rem_reg <= shifted_rem;
        end
        q_reg <= {q_reg[WIDTH - 2:0], next_q_bit};
        dvd <= dvd << 1;
        count <= WIDTH'(count + 1);
        if (count == WIDTH'(WIDTH - 1)) begin
          running <= 1'b0;
          valid <= 1'b1;
          quotient <= {q_reg[WIDTH - 2:0], next_q_bit};
          if (~sub_result[WIDTH]) begin
            remainder <= sub_result[WIDTH - 1:0];
          end else begin
            remainder <= shifted_rem[WIDTH - 1:0];
          end
        end
      end
    end
  end

endmodule

