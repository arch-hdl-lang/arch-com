// E203 HBirdv2 Multiply/Divide Unit — FSM version
// Iterative 32-cycle multiply (shift-add) and 32-cycle restoring divide.
// Supports RV32M: MUL, MULH, MULHSU, MULHU, DIV, DIVU, REM, REMU.
// Valid/ready handshake: accepts when Idle, produces result in Done.
module e203_exu_muldiv_fsm #(
  parameter int XLEN = 32
) (
  input logic clk,
  input logic rst_n,
  input logic i_valid,
  output logic i_ready,
  input logic [32-1:0] i_rs1,
  input logic [32-1:0] i_rs2,
  input logic i_mul,
  input logic i_mulh,
  input logic i_mulhsu,
  input logic i_mulhu,
  input logic i_div,
  input logic i_divu,
  input logic i_rem,
  input logic i_remu,
  output logic o_valid,
  input logic o_ready,
  output logic [32-1:0] o_wdat
);

  typedef enum logic [1:0] {
    IDLE = 2'd0,
    EXEC = 2'd1,
    DONE = 2'd2
  } e203_exu_muldiv_fsm_state_t;
  
  e203_exu_muldiv_fsm_state_t state_r, state_next;
  
  logic [6-1:0] cyc_r;
  logic [32-1:0] acc_hi_r;
  logic [32-1:0] acc_lo_r;
  logic [32-1:0] mcand_r;
  logic [33-1:0] rem_r;
  logic [32-1:0] quot_r;
  logic [32-1:0] dvsr_r;
  logic is_mul_r;
  logic want_hi_r;
  logic want_rem_r;
  logic neg_res_r;
  logic div_zero_r;
  
  logic rs1_sign;
  assign rs1_sign = i_rs1 >> 31 != 0;
  logic rs2_sign;
  assign rs2_sign = i_rs2 >> 31 != 0;
  logic rs1_is_signed;
  assign rs1_is_signed = i_mul | i_mulh | i_mulhsu | i_div | i_rem;
  logic rs2_is_signed;
  assign rs2_is_signed = i_mul | i_mulh | i_div | i_rem;
  logic [32-1:0] rs1_neg;
  assign rs1_neg = 32'(~i_rs1 + 1);
  logic [32-1:0] rs2_neg;
  assign rs2_neg = 32'(~i_rs2 + 1);
  logic [32-1:0] op1_mag;
  assign op1_mag = rs1_is_signed & rs1_sign ? rs1_neg : i_rs1;
  logic [32-1:0] op2_mag;
  assign op2_mag = rs2_is_signed & rs2_sign ? rs2_neg : i_rs2;
  logic mul_neg;
  assign mul_neg = i_mul | i_mulh ? rs1_sign ^ rs2_sign : i_mulhsu ? rs1_sign : 1'b0;
  logic div_neg;
  assign div_neg = i_div & (rs1_sign ^ rs2_sign) | i_rem & rs1_sign;
  logic is_mul_op;
  assign is_mul_op = i_mul | i_mulh | i_mulhsu | i_mulhu;
  logic is_div_op;
  assign is_div_op = i_div | i_divu | i_rem | i_remu;
  logic want_hi;
  assign want_hi = i_mulh | i_mulhsu | i_mulhu;
  logic want_rem;
  assign want_rem = i_rem | i_remu;
  logic [33-1:0] add_res;
  assign add_res = 33'(33'($unsigned(acc_hi_r)) + 33'($unsigned(mcand_r)));
  logic [32-1:0] add_lo;
  assign add_lo = 32'(add_res);
  logic add_carry;
  assign add_carry = add_res >> 32 != 0;
  logic [33-1:0] rem_shifted;
  assign rem_shifted = 33'(33'(rem_r << 1) | 33'($unsigned(quot_r[31:31])));
  logic [34-1:0] trial_sub;
  assign trial_sub = 34'(34'($unsigned(rem_shifted)) - 34'($unsigned(dvsr_r)));
  logic trial_neg;
  assign trial_neg = trial_sub >> 33 != 0;
  logic [32-1:0] carry_mask;
  assign carry_mask = add_carry ? 'h80000000 : 0;
  logic [32-1:0] mul_nxt_hi_add;
  assign mul_nxt_hi_add = 32'(add_lo >> 1) | carry_mask;
  logic [32-1:0] mul_nxt_lo_add;
  assign mul_nxt_lo_add = 32'(acc_lo_r >> 1) | 32'($unsigned(add_lo[0:0])) << 31;
  logic [32-1:0] mul_nxt_hi_nop;
  assign mul_nxt_hi_nop = 32'(acc_hi_r >> 1);
  logic [32-1:0] mul_nxt_lo_nop;
  assign mul_nxt_lo_nop = 32'(acc_lo_r >> 1) | 32'($unsigned(acc_hi_r[0:0])) << 31;
  logic [32-1:0] mul_res;
  assign mul_res = want_hi_r ? acc_hi_r : acc_lo_r;
  logic [32-1:0] mul_res_neg;
  assign mul_res_neg = 32'(~mul_res + 1);
  logic [32-1:0] div_raw;
  assign div_raw = want_rem_r ? 32'(rem_r) : quot_r;
  logic [32-1:0] div_res_neg;
  assign div_res_neg = 32'(~div_raw + 1);
  logic [32-1:0] raw_result;
  assign raw_result = is_mul_r ? mul_res : div_raw;
  logic [32-1:0] neg_result;
  assign neg_result = is_mul_r ? mul_res_neg : div_res_neg;
  
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      state_r <= IDLE;
      cyc_r <= 0;
      acc_hi_r <= 0;
      acc_lo_r <= 0;
      mcand_r <= 0;
      rem_r <= 0;
      quot_r <= 0;
      dvsr_r <= 0;
      is_mul_r <= 0;
      want_hi_r <= 0;
      want_rem_r <= 0;
      neg_res_r <= 0;
      div_zero_r <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        IDLE: begin
          // Dispatch interface
          // Operation select (one-hot)
          // Result
          // Datapath registers
          // Operand sign handling
          // Multiply step intermediates
          // Divide step intermediates
          // Multiply next-state values
          // Result selection
          if (i_valid & (is_mul_op | is_div_op)) begin
            cyc_r <= 0;
            is_mul_r <= is_mul_op;
            want_hi_r <= want_hi;
            want_rem_r <= want_rem;
            neg_res_r <= is_mul_op ? mul_neg : div_neg;
            div_zero_r <= i_rs2 == 0 & is_div_op;
            if (is_mul_op) begin
              acc_hi_r <= 0;
              acc_lo_r <= op2_mag;
              mcand_r <= op1_mag;
            end else begin
              rem_r <= i_rs2 == 0 ? 33'($unsigned(op1_mag)) : 0;
              quot_r <= op1_mag;
              dvsr_r <= op2_mag;
            end
          end
        end
        EXEC: begin
          if (is_mul_r) begin
            if (acc_lo_r[0:0] != 0) begin
              acc_hi_r <= mul_nxt_hi_add;
              acc_lo_r <= mul_nxt_lo_add;
            end else begin
              acc_hi_r <= mul_nxt_hi_nop;
              acc_lo_r <= mul_nxt_lo_nop;
            end
          end else if (~trial_neg) begin
            rem_r <= 33'(trial_sub);
            quot_r <= 32'(32'(quot_r << 1) | 1);
          end else begin
            rem_r <= rem_shifted;
            quot_r <= 32'(quot_r << 1);
          end
          if (cyc_r == 31) begin
          end else begin
            cyc_r <= 6'(cyc_r + 1);
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
        if (i_valid & (is_mul_op | is_div_op)) state_next = EXEC;
      end
      EXEC: begin
        if (cyc_r == 31) state_next = DONE;
      end
      DONE: begin
        if (o_ready) state_next = IDLE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    case (state_r)
      IDLE: begin
        i_ready = 1'b1;
      end
      EXEC: begin
      end
      DONE: begin
        o_valid = 1'b1;
        if (div_zero_r & ~is_mul_r) begin
          if (want_rem_r) begin
            o_wdat = 32'(rem_r);
          end else begin
            o_wdat = 'hFFFFFFFF;
          end
        end else if (neg_res_r) begin
          o_wdat = neg_result;
        end else begin
          o_wdat = raw_result;
        end
      end
      default: ;
    endcase
  end

endmodule

