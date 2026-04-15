module signedadder #(
  parameter int DATA_WIDTH = 8
) (
  input logic i_clk,
  input logic i_rst_n,
  input logic i_start,
  input logic i_enable,
  input logic i_mode,
  input logic i_clear,
  input logic signed [DATA_WIDTH-1:0] i_operand_a,
  input logic signed [DATA_WIDTH-1:0] i_operand_b,
  output logic signed [DATA_WIDTH-1:0] o_resultant_sum,
  output logic o_overflow,
  output logic o_ready,
  output logic [1:0] o_status
);

  // FSM states
  logic [1:0] ST_IDLE;
  assign ST_IDLE = 0;
  logic [1:0] ST_LOAD;
  assign ST_LOAD = 1;
  logic [1:0] ST_COMPUTE;
  assign ST_COMPUTE = 2;
  logic [1:0] ST_OUTPUT;
  assign ST_OUTPUT = 3;
  logic [1:0] state_r;
  // Internal registers
  logic signed [DATA_WIDTH-1:0] op_a_r;
  logic signed [DATA_WIDTH-1:0] op_b_r;
  logic mode_r;
  logic signed [DATA_WIDTH-1:0] sum_r;
  logic ovf_r;
  logic ready_r;
  // Compute the result and overflow in comb logic
  logic signed [DATA_WIDTH-1:0] sum_add;
  assign sum_add = DATA_WIDTH'(op_a_r + op_b_r);
  logic signed [DATA_WIDTH-1:0] sum_sub;
  assign sum_sub = DATA_WIDTH'(op_a_r - op_b_r);
  logic signed [DATA_WIDTH-1:0] result_val;
  logic ovf_val;
  always_comb begin
    if (mode_r) begin
      result_val = sum_sub;
      // Overflow: positive - negative = negative, or negative - positive = positive
      ovf_val = op_a_r[DATA_WIDTH - 1] != op_b_r[DATA_WIDTH - 1] && sum_sub[DATA_WIDTH - 1] != op_a_r[DATA_WIDTH - 1];
    end else begin
      result_val = sum_add;
      // Overflow: both same sign, result different sign
      ovf_val = op_a_r[DATA_WIDTH - 1] == op_b_r[DATA_WIDTH - 1] && sum_add[DATA_WIDTH - 1] != op_a_r[DATA_WIDTH - 1];
    end
  end
  always_ff @(posedge i_clk or negedge i_rst_n) begin
    if ((!i_rst_n)) begin
      mode_r <= 1'b0;
      op_a_r <= 0;
      op_b_r <= 0;
      ovf_r <= 1'b0;
      ready_r <= 1'b0;
      state_r <= 0;
      sum_r <= 0;
    end else begin
      if (i_clear) begin
        state_r <= ST_IDLE;
        sum_r <= 0;
        ovf_r <= 1'b0;
        ready_r <= 1'b0;
      end else if (state_r == ST_IDLE) begin
        ready_r <= 1'b0;
        if (i_enable && i_start) begin
          state_r <= ST_LOAD;
        end
      end else if (state_r == ST_LOAD) begin
        op_a_r <= i_operand_a;
        op_b_r <= i_operand_b;
        mode_r <= i_mode;
        state_r <= ST_COMPUTE;
      end else if (state_r == ST_COMPUTE) begin
        sum_r <= result_val;
        ovf_r <= ovf_val;
        state_r <= ST_OUTPUT;
      end else if (state_r == ST_OUTPUT) begin
        ready_r <= 1'b1;
        state_r <= ST_IDLE;
      end
    end
  end
  assign o_resultant_sum = sum_r;
  assign o_overflow = ovf_r;
  assign o_ready = ready_r;
  assign o_status = state_r;

endmodule

