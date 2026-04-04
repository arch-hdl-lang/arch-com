package SecureRegBankPkg;
  typedef enum logic [1:0] {
    LOCKED = 2'd0,
    GOT_CODE0 = 2'd1,
    UNLOCKED = 2'd2
  } LockState;
  
endpackage

module secure_read_write_register_bank #(
  parameter int P_ADDRESS_WIDTH = 8,
  parameter int P_DATA_WIDTH = 8,
  parameter int P_UNLOCK_CODE_0 = 171,
  parameter int P_UNLOCK_CODE_1 = 205
) (
  input logic i_capture_pulse,
  input logic i_rst_n,
  input logic [P_ADDRESS_WIDTH-1:0] i_addr,
  input logic [P_DATA_WIDTH-1:0] i_data_in,
  input logic i_read_write_enable,
  output logic [P_DATA_WIDTH-1:0] o_data_out
);

  typedef enum logic [1:0] {
    LOCKED = 2'd0,
    GOT_CODE0 = 2'd1,
    UNLOCKED = 2'd2
  } secure_read_write_register_bank_state_t;
  
  secure_read_write_register_bank_state_t state_r, state_next;
  
  logic [P_DATA_WIDTH-1:0] mem [256-1:0];
  
  logic is_write;
  assign is_write = i_read_write_enable == 0;
  logic code0_match;
  assign code0_match = i_data_in == P_DATA_WIDTH'(P_UNLOCK_CODE_0);
  logic code1_match;
  assign code1_match = i_data_in == P_DATA_WIDTH'(P_UNLOCK_CODE_1);
  
  always_ff @(posedge i_capture_pulse or negedge i_rst_n) begin
    if ((!i_rst_n)) begin
      state_r <= LOCKED;
      for (int __ri_mem = 0; __ri_mem < 256; __ri_mem++) begin
        mem[__ri_mem] <= 0;
      end
    end else begin
      state_r <= state_next;
      case (state_r)
        LOCKED: begin
          o_data_out <= 0;
          if (is_write) begin
            if (i_addr == 0) begin
              mem[0] <= i_data_in;
            end else if (i_addr == 1) begin
              mem[1] <= i_data_in;
            end
          end
        end
        GOT_CODE0: begin
          o_data_out <= 0;
          if (is_write) begin
            if (i_addr == 0) begin
              mem[0] <= i_data_in;
            end else if (i_addr == 1) begin
              mem[1] <= i_data_in;
            end
          end
        end
        UNLOCKED: begin
          if (is_write) begin
            if (i_addr == 0) begin
              mem[0] <= i_data_in;
              o_data_out <= 0;
            end else if (i_addr == 1) begin
              mem[1] <= i_data_in;
              o_data_out <= 0;
            end else begin
              mem[i_addr] <= i_data_in;
              o_data_out <= 0;
            end
          end else if (i_addr == 0 | i_addr == 1) begin
            o_data_out <= 0;
          end else begin
            o_data_out <= mem[i_addr];
          end
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      LOCKED: begin
        if (is_write & i_addr == 0 & code0_match) state_next = GOT_CODE0;
      end
      GOT_CODE0: begin
        if (is_write & i_addr == 1 & code1_match) state_next = UNLOCKED;
        else if (is_write & i_addr == 1 & ~code1_match) state_next = LOCKED;
        else if (is_write & i_addr != 0 & i_addr != 1) state_next = LOCKED;
        else if (~is_write) state_next = LOCKED;
      end
      UNLOCKED: begin
        state_next = UNLOCKED;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    case (state_r)
      LOCKED: begin
      end
      GOT_CODE0: begin
      end
      UNLOCKED: begin
      end
      default: ;
    endcase
  end

endmodule

