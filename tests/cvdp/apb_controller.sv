module apb_controller (
  input logic clk,
  input logic reset_n,
  input logic select_a_i,
  input logic select_b_i,
  input logic select_c_i,
  input logic [31:0] addr_a_i,
  input logic [31:0] data_a_i,
  input logic [31:0] addr_b_i,
  input logic [31:0] data_b_i,
  input logic [31:0] addr_c_i,
  input logic [31:0] data_c_i,
  input logic apb_pready_i,
  output logic apb_psel_o,
  output logic apb_penable_o,
  output logic apb_pwrite_o,
  output logic [31:0] apb_paddr_o,
  output logic [31:0] apb_pwdata_o
);

  typedef enum logic [1:0] {
    IDLE = 2'd0,
    SETUP = 2'd1,
    ACCESS = 2'd2
  } apb_controller_state_t;
  
  apb_controller_state_t state_r, state_next;
  
  logic r_psel;
  logic r_penable;
  logic r_pwrite;
  logic [31:0] r_paddr;
  logic [31:0] r_pwdata;
  logic [3:0] timeout_cnt;
  
  always_ff @(posedge clk or negedge reset_n) begin
    if ((!reset_n)) begin
      state_r <= IDLE;
      r_psel <= 1'b0;
      r_penable <= 1'b0;
      r_pwrite <= 1'b0;
      r_paddr <= 0;
      r_pwdata <= 0;
      timeout_cnt <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        IDLE: begin
          if (select_a_i) begin
            r_psel <= 1'b1;
            r_pwrite <= 1'b1;
            r_paddr <= addr_a_i;
            r_pwdata <= data_a_i;
            r_penable <= 1'b0;
          end else if (select_b_i) begin
            r_psel <= 1'b1;
            r_pwrite <= 1'b1;
            r_paddr <= addr_b_i;
            r_pwdata <= data_b_i;
            r_penable <= 1'b0;
          end else if (select_c_i) begin
            r_psel <= 1'b1;
            r_pwrite <= 1'b1;
            r_paddr <= addr_c_i;
            r_pwdata <= data_c_i;
            r_penable <= 1'b0;
          end else begin
            r_psel <= 1'b0;
            r_penable <= 1'b0;
            r_pwrite <= 1'b0;
            r_paddr <= 0;
            r_pwdata <= 0;
          end
          timeout_cnt <= 0;
        end
        SETUP: begin
          r_penable <= 1'b1;
        end
        ACCESS: begin
          if (apb_pready_i) begin
            r_psel <= 1'b0;
            r_penable <= 1'b0;
            r_pwrite <= 1'b0;
            r_paddr <= 0;
            r_pwdata <= 0;
            timeout_cnt <= 0;
          end else if (timeout_cnt == 15) begin
            r_psel <= 1'b0;
            r_penable <= 1'b0;
            r_pwrite <= 1'b0;
            r_paddr <= 0;
            r_pwdata <= 0;
            timeout_cnt <= 0;
          end else begin
            timeout_cnt <= 4'(timeout_cnt + 1);
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
        if (select_a_i | select_b_i | select_c_i) state_next = SETUP;
      end
      SETUP: begin
        state_next = ACCESS;
      end
      ACCESS: begin
        if (apb_pready_i | (timeout_cnt == 15)) state_next = IDLE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    apb_psel_o = r_psel;
    apb_penable_o = r_penable;
    apb_pwrite_o = r_pwrite;
    apb_paddr_o = r_paddr;
    apb_pwdata_o = r_pwdata;
    case (state_r)
      IDLE: begin
      end
      SETUP: begin
      end
      ACCESS: begin
      end
      default: ;
    endcase
  end
  
  // synopsys translate_off
  _auto_legal_state: assert property (@(posedge clk) reset_n |-> state_r < 3)
    else $fatal(1, "FSM ILLEGAL STATE: apb_controller.state_r = %0d", state_r);
  _auto_reach_Idle: cover property (@(posedge clk) state_r == IDLE);
  _auto_reach_Setup: cover property (@(posedge clk) state_r == SETUP);
  _auto_reach_Access: cover property (@(posedge clk) state_r == ACCESS);
  _auto_tr_IDLE_to_SETUP: cover property (@(posedge clk) state_r == IDLE && state_next == SETUP);
  _auto_tr_SETUP_to_ACCESS: cover property (@(posedge clk) state_r == SETUP && state_next == ACCESS);
  _auto_tr_ACCESS_to_IDLE: cover property (@(posedge clk) state_r == ACCESS && state_next == IDLE);
  // synopsys translate_on

endmodule

