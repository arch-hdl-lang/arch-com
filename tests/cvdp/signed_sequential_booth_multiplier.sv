module signed_sequential_booth_multiplier #(
  parameter int WIDTH = 8,
  parameter int HALF = WIDTH / 2
) (
  input logic clk,
  input logic rst,
  input logic start,
  input logic signed [WIDTH-1:0] A,
  input logic signed [WIDTH-1:0] B,
  output logic signed [2 * WIDTH-1:0] result,
  output logic done
);

  typedef enum logic [1:0] {
    IDLE = 2'd0,
    COMPUTE = 2'd1,
    DONE = 2'd2
  } signed_sequential_booth_multiplier_state_t;
  
  signed_sequential_booth_multiplier_state_t state_r, state_next;
  
  logic signed [2 * WIDTH-1:0] accum;
  logic signed [WIDTH-1:0] mcand;
  logic signed [WIDTH-1:0] mplier;
  logic [4-1:0] step;
  
  logic [9-1:0] mplier_ext;
  assign mplier_ext = {mplier, 1'd0};
  logic [4-1:0] shift_amt;
  assign shift_amt = step * 2;
  logic [9-1:0] shifted9;
  assign shifted9 = mplier_ext >> shift_amt;
  logic [3-1:0] grp3;
  assign grp3 = shifted9[2:0];
  logic signed [2 * WIDTH-1:0] m_ext;
  assign m_ext = {{(2 * WIDTH-$bits(mcand)){mcand[$bits(mcand)-1]}}, mcand};
  logic signed [2 * WIDTH-1:0] m2_pos;
  assign m2_pos = m_ext << 1;
  logic signed [2 * WIDTH-1:0] zero16;
  assign zero16 = accum ^ accum;
  logic signed [2 * WIDTH + 1-1:0] m_neg;
  assign m_neg = {{(2 * WIDTH + 1-$bits(zero16)){zero16[$bits(zero16)-1]}}, zero16} - {{(2 * WIDTH + 1-$bits(m_ext)){m_ext[$bits(m_ext)-1]}}, m_ext};
  logic signed [2 * WIDTH + 1-1:0] m2_neg;
  assign m2_neg = {{(2 * WIDTH + 1-$bits(zero16)){zero16[$bits(zero16)-1]}}, zero16} - {{(2 * WIDTH + 1-$bits(m2_pos)){m2_pos[$bits(m2_pos)-1]}}, m2_pos};
  logic is_zero;
  assign is_zero = grp3 == 0 | grp3 == 7;
  logic is_pos1;
  assign is_pos1 = grp3 == 1 | grp3 == 2;
  logic is_pos2;
  assign is_pos2 = grp3 == 3;
  logic is_neg2;
  assign is_neg2 = grp3 == 4;
  logic signed [2 * WIDTH + 1-1:0] pp_sel;
  assign pp_sel = is_zero ? {{(2 * WIDTH + 1-$bits(zero16)){zero16[$bits(zero16)-1]}}, zero16} : is_pos1 ? {{(2 * WIDTH + 1-$bits(m_ext)){m_ext[$bits(m_ext)-1]}}, m_ext} : is_pos2 ? {{(2 * WIDTH + 1-$bits(m2_pos)){m2_pos[$bits(m2_pos)-1]}}, m2_pos} : is_neg2 ? m2_neg : m_neg;
  logic signed [2 * WIDTH + 1-1:0] pp_shifted;
  assign pp_shifted = pp_sel << shift_amt;
  logic signed [2 * WIDTH + 2-1:0] accum_next;
  assign accum_next = {{(2 * WIDTH + 2-$bits(accum)){accum[$bits(accum)-1]}}, accum} + {{(2 * WIDTH + 2-$bits(pp_shifted)){pp_shifted[$bits(pp_shifted)-1]}}, pp_shifted};
  logic [4-1:0] last_step;
  assign last_step = 3;
  
  always_ff @(posedge clk or posedge rst) begin
    if (rst) begin
      state_r <= IDLE;
      accum <= 0;
      mcand <= 0;
      mplier <= 0;
      step <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        IDLE: begin
          // Extended multiplier with appended 0 at bit -1: {mplier, 1'b0}
          // Shift mplier_ext right then take bottom 3 bits
          // Booth encoding partial products
          // Last step index as UInt<4>
          if (start) begin
            accum <= zero16;
            mcand <= A;
            mplier <= B;
            step <= 0;
          end
        end
        COMPUTE: begin
          accum <= (2 * WIDTH)'(accum_next);
          step <= step + 1;
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      IDLE: begin
        if (start) state_next = COMPUTE;
      end
      COMPUTE: begin
        if (step == last_step) state_next = DONE;
      end
      DONE: begin
        if (start == 1'b0) state_next = IDLE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    case (state_r)
      IDLE: begin
        result = accum;
        done = 1'b0;
      end
      COMPUTE: begin
        result = accum;
        done = 1'b0;
      end
      DONE: begin
        result = accum;
        done = 1'b1;
      end
      default: ;
    endcase
  end

endmodule

