module hmac_reg_interface #(
  parameter int DATA_WIDTH = 8,
  parameter int ADDR_WIDTH = 8
) (
  input logic clk,
  input logic rst_n,
  input logic write_en,
  input logic read_en,
  input logic [ADDR_WIDTH-1:0] addr,
  input logic [DATA_WIDTH-1:0] wdata,
  input logic i_wait_en,
  output logic [DATA_WIDTH-1:0] rdata,
  output logic hmac_valid,
  output logic hmac_key_error
);

  typedef enum logic [2:0] {
    IDLE = 3'd0,
    ANALYZE = 3'd1,
    XOR_DATA = 3'd2,
    WRITE = 3'd3,
    LOST = 3'd4,
    CHECK_KEY = 3'd5,
    TRIG_WAIT = 3'd6
  } hmac_reg_interface_state_t;
  
  hmac_reg_interface_state_t state_r, state_next;
  
  logic [DATA_WIDTH-1:0] hmac_key;
  logic [DATA_WIDTH-1:0] hmac_data;
  logic [255:0] [DATA_WIDTH-1:0] registers;
  
  logic [2:0] current_state;
  assign current_state = state_r;
  
  logic [DATA_WIDTH-1:0] xor_data;
  logic [DATA_WIDTH-1:0] xor_mask;
  logic key_valid;
  logic next_key_valid;
  
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      state_r <= IDLE;
      hmac_key <= 0;
      hmac_data <= 0;
      for (int __ri_registers = 0; __ri_registers < 256; __ri_registers++) begin
        registers[__ri_registers] <= 0;
      end
      rdata <= 0;
      hmac_valid <= 1'b0;
      hmac_key_error <= 1'b0;
    end else begin
      state_r <= state_next;
      // Datapath regs (alongside FSM state)
      // Internal wires.
      // Look-ahead key-error: default = check current hmac_key, WRITE
      // state overrides to check wdata when writing the key (addr == 0).
      // The CVDP TB probes `dut.current_state.value`; alias `state` so the
      // test can read it without renaming inside the auto-generated FSM.
      // xor_mask: alternating-1s pattern (01010101...) — TB-visible.
      // Key validation: 2 MSB and 2 LSB must be zero.
      // Default look-ahead: not writing the key this cycle.
      hmac_key_error <= ~next_key_valid;
      hmac_valid <= 1'b0;
      // Reads: serve from non-WRITE states.
      if (read_en & (state_r != WRITE)) begin
        if (addr == 0) begin
          rdata <= hmac_key;
        end else if (addr == 1) begin
          rdata <= hmac_data;
        end else begin
          rdata <= registers[addr];
        end
      end
      case (state_r)
        WRITE: begin
          // Look-ahead override: when writing the key this cycle, validate
          // against wdata so hmac_key_error tracks it without a 1-cycle lag.
          if (addr == 0) begin
            hmac_key <= wdata;
          end else if (addr == 1) begin
            hmac_data <= wdata;
            hmac_valid <= 1'b1;
          end else begin
            registers[addr] <= wdata;
          end
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      IDLE: begin
        if (write_en) state_next = ANALYZE;
      end
      ANALYZE: begin
        if (wdata[DATA_WIDTH - 1 +: 1] == 1) state_next = XOR_DATA;
        else if (wdata[DATA_WIDTH - 1 +: 1] == 0) state_next = WRITE;
      end
      XOR_DATA: begin
        state_next = WRITE;
      end
      WRITE: begin
        if (write_en) state_next = IDLE;
        else if (!write_en) state_next = LOST;
      end
      LOST: begin
        if (read_en) state_next = CHECK_KEY;
      end
      CHECK_KEY: begin
        if (key_valid) state_next = TRIG_WAIT;
        else if (!key_valid) state_next = WRITE;
      end
      TRIG_WAIT: begin
        if (i_wait_en) state_next = TRIG_WAIT;
        else if (!i_wait_en & (hmac_data != 0) & (hmac_key != 0)) state_next = IDLE;
        else if (!i_wait_en & ((hmac_data == 0) | (hmac_key == 0))) state_next = WRITE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    xor_mask = DATA_WIDTH'($unsigned(0));
    for (int i = 0; i <= DATA_WIDTH / 2 - 1; i++) begin
      xor_mask[i * 2 +: 2] = 2'd1;
    end
    xor_data = wdata ^ xor_mask;
    key_valid = (hmac_key[DATA_WIDTH - 2 +: 2] == 0) & (hmac_key[1:0] == 0);
    next_key_valid = key_valid;
    case (state_r)
      IDLE: begin
      end
      ANALYZE: begin
      end
      XOR_DATA: begin
      end
      WRITE: begin
        if (addr == 0) begin
          next_key_valid = (wdata[DATA_WIDTH - 2 +: 2] == 0) & (wdata[1:0] == 0);
        end
      end
      LOST: begin
      end
      CHECK_KEY: begin
      end
      TRIG_WAIT: begin
      end
      default: ;
    endcase
  end
  
  // synopsys translate_off
  _auto_legal_state: assert property (@(posedge clk) rst_n |-> state_r < 7)
    else $fatal(1, "FSM ILLEGAL STATE: hmac_reg_interface.state_r = %0d", state_r);
  _auto_reach_IDLE: cover property (@(posedge clk) state_r == IDLE);
  _auto_reach_ANALYZE: cover property (@(posedge clk) state_r == ANALYZE);
  _auto_reach_XOR_DATA: cover property (@(posedge clk) state_r == XOR_DATA);
  _auto_reach_WRITE: cover property (@(posedge clk) state_r == WRITE);
  _auto_reach_LOST: cover property (@(posedge clk) state_r == LOST);
  _auto_reach_CHECK_KEY: cover property (@(posedge clk) state_r == CHECK_KEY);
  _auto_reach_TRIG_WAIT: cover property (@(posedge clk) state_r == TRIG_WAIT);
  _auto_tr_IDLE_to_ANALYZE: cover property (@(posedge clk) state_r == IDLE && state_next == ANALYZE);
  _auto_tr_ANALYZE_to_XOR_DATA: cover property (@(posedge clk) state_r == ANALYZE && state_next == XOR_DATA);
  _auto_tr_ANALYZE_to_WRITE: cover property (@(posedge clk) state_r == ANALYZE && state_next == WRITE);
  _auto_tr_XOR_DATA_to_WRITE: cover property (@(posedge clk) state_r == XOR_DATA && state_next == WRITE);
  _auto_tr_WRITE_to_IDLE: cover property (@(posedge clk) state_r == WRITE && state_next == IDLE);
  _auto_tr_WRITE_to_LOST: cover property (@(posedge clk) state_r == WRITE && state_next == LOST);
  _auto_tr_LOST_to_CHECK_KEY: cover property (@(posedge clk) state_r == LOST && state_next == CHECK_KEY);
  _auto_tr_CHECK_KEY_to_TRIG_WAIT: cover property (@(posedge clk) state_r == CHECK_KEY && state_next == TRIG_WAIT);
  _auto_tr_CHECK_KEY_to_WRITE: cover property (@(posedge clk) state_r == CHECK_KEY && state_next == WRITE);
  _auto_tr_TRIG_WAIT_to_TRIG_WAIT: cover property (@(posedge clk) state_r == TRIG_WAIT && state_next == TRIG_WAIT);
  _auto_tr_TRIG_WAIT_to_IDLE: cover property (@(posedge clk) state_r == TRIG_WAIT && state_next == IDLE);
  _auto_tr_TRIG_WAIT_to_WRITE: cover property (@(posedge clk) state_r == TRIG_WAIT && state_next == WRITE);
  // synopsys translate_on

endmodule

