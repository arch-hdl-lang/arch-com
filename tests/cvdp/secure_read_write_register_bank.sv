module secure_read_write_register_bank #(
  parameter int p_address_width = 8,
  parameter int p_data_width = 8,
  parameter int p_unlock_code_0 = 'hAB,
  parameter int p_unlock_code_1 = 'hCD,
  parameter int MEM_DEPTH = 1 << p_address_width
) (
  input logic i_capture_pulse,
  input logic i_rst_n,
  input logic [p_address_width-1:0] i_addr,
  input logic [p_data_width-1:0] i_data_in,
  input logic i_read_write_enable,
  output logic [p_data_width-1:0] o_data_out
);

  typedef enum logic [1:0] {
    LOCKED = 2'd0,
    CODE0WRITTEN = 2'd1,
    UNLOCKED = 2'd2
  } secure_read_write_register_bank_state_t;
  
  secure_read_write_register_bank_state_t state_r, state_next;
  
  logic [MEM_DEPTH-1:0] [p_data_width-1:0] mem;
  logic [p_data_width-1:0] data_out_r;
  
  logic is_write;
  assign is_write = ~i_read_write_enable;
  logic addr_is_0;
  assign addr_is_0 = i_addr == 0;
  logic addr_is_1;
  assign addr_is_1 = i_addr == 1;
  logic addr_is_other;
  assign addr_is_other = ~addr_is_0 & ~addr_is_1;
  logic match_code0;
  assign match_code0 = i_data_in == p_data_width'(p_unlock_code_0);
  logic match_code1;
  assign match_code1 = i_data_in == p_data_width'(p_unlock_code_1);
  
  always_ff @(posedge i_capture_pulse or negedge i_rst_n) begin
    if ((!i_rst_n)) begin
      state_r <= LOCKED;
      for (int __ri_mem = 0; __ri_mem < MEM_DEPTH; __ri_mem++) begin
        mem[__ri_mem] <= 0;
      end
      data_out_r <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        LOCKED: begin
          data_out_r <= 0;
        end
        CODE0WRITTEN: begin
          data_out_r <= 0;
        end
        UNLOCKED: begin
          if (is_write) begin
            if (addr_is_other) begin
              mem[i_addr] <= i_data_in;
            end
            data_out_r <= 0;
          end else if (addr_is_other) begin
            data_out_r <= mem[i_addr];
          end else begin
            data_out_r <= 0;
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
        if (is_write & addr_is_0 & match_code0) state_next = CODE0WRITTEN;
      end
      CODE0WRITTEN: begin
        if (is_write & addr_is_1 & match_code1) state_next = UNLOCKED;
        else if (~(is_write & addr_is_1 & match_code1)) state_next = LOCKED;
      end
      UNLOCKED: begin
        if (is_write & addr_is_0 & match_code0) state_next = CODE0WRITTEN;
        else if (is_write & addr_is_0 & ~match_code0) state_next = LOCKED;
        else if (is_write & addr_is_1) state_next = LOCKED;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    o_data_out = data_out_r;
    case (state_r)
      LOCKED: begin
      end
      CODE0WRITTEN: begin
      end
      UNLOCKED: begin
      end
      default: ;
    endcase
  end

endmodule

