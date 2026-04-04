module fifo_policy #(
  parameter int NWAYS = 4,
  parameter int NINDEXES = 8
) (
  input logic clock,
  input logic reset,
  input logic [3-1:0] index,
  input logic [2-1:0] way_select,
  input logic access,
  input logic hit,
  output logic [2-1:0] way_replace
);

  logic [2-1:0] fifo_array_0;
  logic [2-1:0] fifo_array_1;
  logic [2-1:0] fifo_array_2;
  logic [2-1:0] fifo_array_3;
  logic [2-1:0] fifo_array_4;
  logic [2-1:0] fifo_array_5;
  logic [2-1:0] fifo_array_6;
  logic [2-1:0] fifo_array_7;
  logic [2-1:0] cur_val;
  logic [2-1:0] next_val;
  always_comb begin
    if (index == 0) begin
      cur_val = fifo_array_0;
    end else if (index == 1) begin
      cur_val = fifo_array_1;
    end else if (index == 2) begin
      cur_val = fifo_array_2;
    end else if (index == 3) begin
      cur_val = fifo_array_3;
    end else if (index == 4) begin
      cur_val = fifo_array_4;
    end else if (index == 5) begin
      cur_val = fifo_array_5;
    end else if (index == 6) begin
      cur_val = fifo_array_6;
    end else begin
      cur_val = fifo_array_7;
    end
    way_replace = cur_val;
    if (cur_val == 3) begin
      next_val = 0;
    end else begin
      next_val = 2'(cur_val + 1);
    end
  end
  always_ff @(posedge clock or posedge reset) begin
    if (reset) begin
      fifo_array_0 <= 0;
      fifo_array_1 <= 0;
      fifo_array_2 <= 0;
      fifo_array_3 <= 0;
      fifo_array_4 <= 0;
      fifo_array_5 <= 0;
      fifo_array_6 <= 0;
      fifo_array_7 <= 0;
    end else begin
      if (access & ~hit) begin
        if (index == 0) begin
          fifo_array_0 <= next_val;
        end else if (index == 1) begin
          fifo_array_1 <= next_val;
        end else if (index == 2) begin
          fifo_array_2 <= next_val;
        end else if (index == 3) begin
          fifo_array_3 <= next_val;
        end else if (index == 4) begin
          fifo_array_4 <= next_val;
        end else if (index == 5) begin
          fifo_array_5 <= next_val;
        end else if (index == 6) begin
          fifo_array_6 <= next_val;
        end else begin
          fifo_array_7 <= next_val;
        end
      end
    end
  end

endmodule

