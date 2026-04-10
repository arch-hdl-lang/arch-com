module data_serializer #(
  parameter int DATA_W = 8,
  parameter int BIT_ORDER = 0,
  parameter int PARITY = 0,
  localparam int EXTRA_BIT = PARITY != 0 ? 1 : 0,
  localparam int SHIFT_W = DATA_W + EXTRA_BIT,
  localparam int CNT_W = $clog2(SHIFT_W) + 1
) (
  input logic clk,
  input logic reset,
  input logic p_valid_i,
  input logic [DATA_W-1:0] p_data_i,
  input logic s_ready_i,
  input logic tx_en_i,
  output logic p_ready_o,
  output logic s_valid_o,
  output logic s_data_o
);

  typedef enum logic [0:0] {
    STRX = 1'd0,
    STTX = 1'd1
  } data_serializer_state_t;
  
  data_serializer_state_t state_r, state_next;
  
  logic [SHIFT_W-1:0] shift_reg_q;
  logic [CNT_W-1:0] bit_cnt_r;
  
  logic parity_even;
  assign parity_even = ^p_data_i;
  logic parity_bit;
  assign parity_bit = PARITY == 2 ? ~parity_even : parity_even;
  logic [SHIFT_W-1:0] load_val;
  assign load_val = BIT_ORDER == 0 ? PARITY != 0 ? SHIFT_W'({parity_bit, p_data_i}) : SHIFT_W'($unsigned(p_data_i)) : PARITY != 0 ? SHIFT_W'({parity_bit, p_data_i[0], p_data_i[1], p_data_i[2], p_data_i[3], p_data_i[4], p_data_i[5], p_data_i[6], p_data_i[7]}) : SHIFT_W'({p_data_i[0], p_data_i[1], p_data_i[2], p_data_i[3], p_data_i[4], p_data_i[5], p_data_i[6], p_data_i[7]});
  logic [CNT_W-1:0] cnt_max;
  assign cnt_max = CNT_W'(SHIFT_W - 1);
  
  always_ff @(posedge clk) begin
    if (reset) begin
      state_r <= STRX;
      shift_reg_q <= 0;
      bit_cnt_r <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        STRX: begin
          // Compute parity: XOR reduction of p_data_i
          // Build the shift register load value — always shift out bit[0] first.
          // LSB-first (BIT_ORDER==0): bit[0]=data[0], ..., bit[7]=data[7], bit[8]=parity
          //   => load data as-is in lower bits, parity in MSB
          // MSB-first (BIT_ORDER==1): bit[0]=data[7], ..., bit[7]=data[0], bit[8]=parity
          //   => reverse data bits in lower positions, parity in MSB
          // Terminal count for comparison
          if (p_valid_i) begin
            shift_reg_q <= load_val;
            bit_cnt_r <= 0;
          end
        end
        STTX: begin
          if (s_ready_i && tx_en_i) begin
            shift_reg_q <= {1'd0, shift_reg_q[SHIFT_W - 1:1]};
            bit_cnt_r <= bit_cnt_r + 1;
          end
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      STRX: begin
        if (p_valid_i) state_next = STTX;
      end
      STTX: begin
        if (s_ready_i && tx_en_i && bit_cnt_r == cnt_max) state_next = STRX;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    p_ready_o = 1'b0;
    s_valid_o = 1'b0;
    s_data_o = 1'b0;
    case (state_r)
      STRX: begin
        p_ready_o = 1'b1;
      end
      STTX: begin
        s_valid_o = 1'b1;
        s_data_o = shift_reg_q[0];
      end
      default: ;
    endcase
  end

endmodule

