module divider #(
  parameter int WIDTH = 32,
  parameter int AW = WIDTH + 1,
  parameter int AQW = WIDTH + WIDTH + 1
) (
  input logic clk,
  input logic rst_n,
  input logic start,
  input logic [WIDTH-1:0] dividend,
  input logic [WIDTH-1:0] divisor,
  output logic [WIDTH-1:0] quotient,
  output logic [WIDTH-1:0] remainder,
  output logic valid
);

  // FSM states as constants
  logic [1:0] ST_IDLE;
  assign ST_IDLE = 0;
  logic [1:0] ST_BUSY;
  assign ST_BUSY = 1;
  logic [1:0] ST_DONE;
  assign ST_DONE = 2;
  logic [1:0] state_r;
  // AQ combined register: A is top AW bits, Q is bottom WIDTH bits
  logic [AQW-1:0] aq_r;
  logic [AW-1:0] m_r;
  logic [WIDTH-1:0] n_r;
  logic [WIDTH-1:0] quotient_r;
  logic [WIDTH-1:0] remainder_r;
  logic valid_r;
  // Shift AQ left by 1: take top AQW-1 bits and append 0
  logic [AW-1:0] a_shifted;
  assign a_shifted = aq_r[AQW - 2:WIDTH - 1];
  logic [WIDTH-1:0] q_shifted;
  assign q_shifted = {aq_r[WIDTH - 2:0], 1'd0};
  // Current A (top bits of aq_r) for sign check
  logic [AW-1:0] a_current;
  assign a_current = aq_r[AQW - 1:WIDTH];
  // Add and subtract with trunc to stay at AW
  logic [AW-1:0] a_plus_m;
  assign a_plus_m = AW'(a_shifted + m_r);
  logic [AW-1:0] a_minus_m;
  assign a_minus_m = AW'(a_shifted - m_r);
  // New A after operation based on current A sign
  logic [AW-1:0] a_new;
  logic [0:0] q_new_lsb;
  always_comb begin
    if (a_current[WIDTH]) begin
      a_new = a_plus_m;
    end else begin
      a_new = a_minus_m;
    end
    if (a_new[WIDTH]) begin
      q_new_lsb = 0;
    end else begin
      q_new_lsb = 1;
    end
  end
  // Final adjustment for remainder
  logic [AW-1:0] a_final_adj;
  assign a_final_adj = AW'(a_current + m_r);
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      aq_r <= 0;
      m_r <= 0;
      n_r <= 0;
      quotient_r <= 0;
      remainder_r <= 0;
      state_r <= 0;
      valid_r <= 1'b0;
    end else begin
      if (state_r == ST_IDLE) begin
        valid_r <= 1'b0;
        if (start) begin
          aq_r <= AQW'($unsigned(dividend));
          m_r <= AW'($unsigned(divisor));
          n_r <= WIDTH'($unsigned(WIDTH - 1));
          state_r <= ST_BUSY;
        end
      end else if (state_r == ST_BUSY) begin
        aq_r <= {a_new, q_shifted[WIDTH - 1:1], q_new_lsb};
        n_r <= WIDTH'(n_r - 1);
        if (n_r == 0) begin
          state_r <= ST_DONE;
        end
      end else if (state_r == ST_DONE) begin
        if (a_current[WIDTH]) begin
          quotient_r <= aq_r[WIDTH - 1:0];
          remainder_r <= a_final_adj[WIDTH - 1:0];
        end else begin
          quotient_r <= aq_r[WIDTH - 1:0];
          remainder_r <= a_current[WIDTH - 1:0];
        end
        valid_r <= 1'b1;
        if (~start) begin
          state_r <= ST_IDLE;
        end
      end
    end
  end
  assign quotient = quotient_r;
  assign remainder = remainder_r;
  assign valid = valid_r;

endmodule

