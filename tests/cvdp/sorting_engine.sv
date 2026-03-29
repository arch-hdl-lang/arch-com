module sorting_engine #(
  parameter int N = 8,
  parameter int WIDTH = 8
) (
  input logic clk,
  input logic rst,
  input logic start,
  input logic [N * WIDTH-1:0] in_data,
  output logic done,
  output logic [N * WIDTH-1:0] out_data
);

  // FSM states: 0=IDLE, 1=SORTING, 2=DONE
  logic [2-1:0] state;
  logic r_done;
  // Sort array stored as flat register
  logic [N * WIDTH-1:0] arr = 0;
  // Iteration counters
  logic [32-1:0] pass_cnt;
  logic [32-1:0] idx;
  // Extract elements at idx and idx+1 for comparison
  logic [WIDTH-1:0] elem_a;
  logic [WIDTH-1:0] elem_b;
  logic [N * WIDTH-1:0] swapped_arr;
  always_comb begin
    elem_a = arr[idx * WIDTH +: WIDTH];
    elem_b = arr[(idx + 1) * WIDTH +: WIDTH];
    swapped_arr = arr;
    if (elem_a > elem_b) begin
      swapped_arr[idx * WIDTH +: WIDTH] = elem_b;
      swapped_arr[(idx + 1) * WIDTH +: WIDTH] = elem_a;
    end
  end
  // Build array with potential swap
  always_ff @(posedge clk or posedge rst) begin
    if (rst) begin
      idx <= 0;
      pass_cnt <= 0;
      r_done <= 1'b0;
      state <= 0;
    end else begin
      if (state == 0) begin
        r_done <= 1'b0;
        if (start) begin
          arr <= in_data;
          pass_cnt <= 0;
          idx <= 0;
          state <= 1;
        end
      end else if (state == 1) begin
        arr <= swapped_arr;
        if (idx == 32'(N - 2)) begin
          idx <= 0;
          pass_cnt <= 32'(pass_cnt + 1);
          if (32'(pass_cnt + 1) == 32'(N)) begin
            state <= 2;
          end
        end else begin
          idx <= 32'(idx + 1);
        end
      end else if (state == 2) begin
        r_done <= 1'b1;
        state <= 0;
      end else begin
        state <= 0;
      end
    end
  end
  // IDLE
  // SORTING: one comparison per cycle
  // DONE
  assign done = r_done;
  assign out_data = arr;

endmodule

