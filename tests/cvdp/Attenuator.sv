// SPI-style attenuator controller: shifts 5-bit data out serially
// when input changes, with clock gating and latch-enable pulse.
module Attenuator (
  input logic clk,
  input logic reset,
  input logic [5-1:0] data,
  output logic [1-1:0] ATTN_CLK,
  output logic [1-1:0] ATTN_DATA,
  output logic [1-1:0] ATTN_LE
);

  typedef enum logic [1:0] {
    IDLE = 2'd0,
    LOAD = 2'd1,
    SHIFT = 2'd2,
    LATCH = 2'd3
  } Attenuator_state_t;
  
  Attenuator_state_t state_r, state_next;
  
  logic [1-1:0] clk_div2;
  logic [5-1:0] shift_reg;
  logic [3-1:0] bit_cnt;
  logic [5-1:0] old_data;
  logic [1-1:0] attn_clk_r;
  logic [1-1:0] attn_data_r;
  logic [1-1:0] attn_le_r;
  
  logic [1-1:0] zero1;
  assign zero1 = 0;
  
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      state_r <= IDLE;
      clk_div2 <= 0;
      shift_reg <= 0;
      bit_cnt <= 0;
      old_data <= 0;
      attn_clk_r <= 0;
      attn_data_r <= 0;
      attn_le_r <= 0;
    end else begin
      state_r <= state_next;
      clk_div2 <= ~clk_div2;
      case (state_r)
        IDLE: begin
          attn_clk_r <= 0;
          attn_data_r <= 0;
          attn_le_r <= 0;
          if (data != old_data) begin
            old_data <= data;
          end
        end
        LOAD: begin
          shift_reg <= data;
          bit_cnt <= 0;
          attn_clk_r <= 0;
          attn_data_r <= 0;
          attn_le_r <= 0;
        end
        SHIFT: begin
          if (clk_div2 == 1) begin
            attn_data_r <= shift_reg[4:4];
            attn_clk_r <= 1;
            shift_reg <= {shift_reg[3:0], zero1};
            if (bit_cnt == 4) begin
              bit_cnt <= 0;
            end else begin
              bit_cnt <= 3'(bit_cnt + 1);
            end
          end else begin
            attn_clk_r <= 0;
          end
        end
        LATCH: begin
          attn_clk_r <= 0;
          attn_data_r <= 0;
          attn_le_r <= 1;
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      IDLE: begin
        if (data != old_data) state_next = LOAD;
      end
      LOAD: begin
        state_next = SHIFT;
      end
      SHIFT: begin
        if (clk_div2 == 1 && bit_cnt == 4) state_next = LATCH;
      end
      LATCH: begin
        state_next = IDLE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    ATTN_CLK = attn_clk_r;
    ATTN_DATA = attn_data_r;
    ATTN_LE = attn_le_r;
    case (state_r)
      IDLE: begin
      end
      LOAD: begin
      end
      SHIFT: begin
      end
      LATCH: begin
      end
      default: ;
    endcase
  end

endmodule

