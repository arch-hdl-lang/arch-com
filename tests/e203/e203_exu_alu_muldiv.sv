// E203 MUL/DIV Sub-unit (Iterative, Shared Adder)
// 17-cycle Booth-4 multiply, 33-cycle non-restoring divide + correction.
// Uses shared 35-bit adder and two 33-bit buffers from the ALU top.
module e203_exu_alu_muldiv (
  input logic clk,
  input logic rst_n,
  input logic mdv_nob2b,
  input logic muldiv_i_valid,
  output logic muldiv_i_ready,
  input logic [32-1:0] muldiv_i_rs1,
  input logic [32-1:0] muldiv_i_rs2,
  input logic [32-1:0] muldiv_i_imm,
  input logic [13-1:0] muldiv_i_info,
  input logic [1-1:0] muldiv_i_itag,
  output logic muldiv_i_longpipe,
  input logic flush_pulse,
  output logic muldiv_o_valid,
  input logic muldiv_o_ready,
  output logic [32-1:0] muldiv_o_wbck_wdat,
  output logic muldiv_o_wbck_err,
  output logic [35-1:0] muldiv_req_alu_op1,
  output logic [35-1:0] muldiv_req_alu_op2,
  output logic muldiv_req_alu_add,
  output logic muldiv_req_alu_sub,
  input logic [35-1:0] muldiv_req_alu_res,
  output logic muldiv_sbf_0_ena,
  output logic [33-1:0] muldiv_sbf_0_nxt,
  input logic [33-1:0] muldiv_sbf_0_r,
  output logic muldiv_sbf_1_ena,
  output logic [33-1:0] muldiv_sbf_1_nxt,
  input logic [33-1:0] muldiv_sbf_1_r
);

  // Dispatch handshake
  // Result handshake
  // Shared 35-bit adder
  // Shared buffers (33-bit each)
  // Decode info fields
  logic i_mul;
  assign i_mul = muldiv_i_info[4:4];
  logic i_mulh;
  assign i_mulh = muldiv_i_info[5:5];
  logic i_mulhsu;
  assign i_mulhsu = muldiv_i_info[6:6];
  logic i_mulhu;
  assign i_mulhu = muldiv_i_info[7:7];
  logic i_div;
  assign i_div = muldiv_i_info[8:8];
  logic i_divu;
  assign i_divu = muldiv_i_info[9:9];
  logic i_rem;
  assign i_rem = muldiv_i_info[10:10];
  logic i_remu;
  assign i_remu = muldiv_i_info[11:11];
  logic i_b2b;
  assign i_b2b = muldiv_i_info[12:12];
  logic is_mul;
  assign is_mul = i_mul | i_mulh | i_mulhsu | i_mulhu;
  logic is_div;
  assign is_div = i_div | i_divu | i_rem | i_remu;
  // Signed handling
  logic mul_rs1_sign;
  assign mul_rs1_sign = i_mulhu ? 1'b0 : muldiv_i_rs1[31:31];
  logic mul_rs2_sign;
  assign mul_rs2_sign = i_mulhsu | i_mulhu ? 1'b0 : muldiv_i_rs2[31:31];
  logic div_rs1_sign;
  assign div_rs1_sign = i_divu | i_remu ? 1'b0 : muldiv_i_rs1[31:31];
  logic div_rs2_sign;
  assign div_rs2_sign = i_divu | i_remu ? 1'b0 : muldiv_i_rs2[31:31];
  // States: 0TH=0, EXEC=1, REMD_CHCK=2, QUOT_CORR=3, REMD_CORR=4
  logic [3-1:0] state_r;
  logic [6-1:0] exec_cnt_r;
  logic flushed_r;
  logic part_prdt_sft1_r;
  logic part_remd_sft1_r;
  logic sta_0th;
  assign sta_0th = state_r == 0;
  logic sta_exec;
  assign sta_exec = state_r == 1;
  logic sta_remd_chck;
  assign sta_remd_chck = state_r == 2;
  logic sta_quot_corr;
  assign sta_quot_corr = state_r == 3;
  logic sta_remd_corr;
  assign sta_remd_corr = state_r == 4;
  logic o_hsked;
  assign o_hsked = muldiv_o_valid & muldiv_o_ready;
  logic back2back_seq;
  assign back2back_seq = i_b2b & ~flushed_r & ~mdv_nob2b;
  // Div special cases
  logic div_by_0;
  assign div_by_0 = muldiv_i_rs2 == 0;
  logic div_ovf;
  assign div_ovf = (i_div | i_rem) & muldiv_i_rs2 == 32'd4294967295 & muldiv_i_rs1[31:31] & muldiv_i_rs1[30:0] == 0;
  logic special_cases;
  assign special_cases = is_div & (div_by_0 | div_ovf);
  logic muldiv_i_valid_nb2b;
  assign muldiv_i_valid_nb2b = muldiv_i_valid & ~back2back_seq & ~special_cases;
  // Cycle counting
  logic cycle_0th;
  assign cycle_0th = sta_0th;
  logic cycle_16th;
  assign cycle_16th = exec_cnt_r == 16;
  logic cycle_32nd;
  assign cycle_32nd = exec_cnt_r == 32;
  logic exec_last;
  assign exec_last = is_mul ? cycle_16th : cycle_32nd;
  // State exit enables
  logic state_0th_exit_ena;
  assign state_0th_exit_ena = sta_0th & muldiv_i_valid_nb2b & ~flush_pulse;
  logic state_exec_exit_ena;
  assign state_exec_exit_ena = sta_exec & (exec_last & (is_div | o_hsked) | flush_pulse);
  logic state_quot_corr_exit_ena;
  assign state_quot_corr_exit_ena = sta_quot_corr;
  logic state_remd_corr_exit_ena;
  assign state_remd_corr_exit_ena = sta_remd_corr & (flush_pulse | o_hsked);
  logic state_exec_enter_ena;
  assign state_exec_enter_ena = state_0th_exit_ena;
  // Aliases to shared buffers
  logic [33-1:0] part_prdt_hi_r;
  assign part_prdt_hi_r = muldiv_sbf_0_r;
  logic [33-1:0] part_prdt_lo_r;
  assign part_prdt_lo_r = muldiv_sbf_1_r;
  logic [33-1:0] part_remd_r;
  assign part_remd_r = muldiv_sbf_0_r;
  logic [33-1:0] part_quot_r;
  assign part_quot_r = muldiv_sbf_1_r;
  // All intermediate wires
  logic div_need_corrct;
  logic state_remd_chck_exit_ena;
  logic [3-1:0] booth_code;
  logic booth_sel_zero;
  logic booth_sel_two;
  logic booth_sel_one;
  logic booth_sel_sub;
  logic [35-1:0] mul_exe_alu_op1;
  logic [35-1:0] mul_exe_alu_op2;
  logic mul_exe_alu_add;
  logic mul_exe_alu_sub;
  logic [66-1:0] dividend;
  logic [34-1:0] divisor;
  logic quot_0cycl;
  logic [67-1:0] dividend_lsft1;
  logic prev_quot;
  logic current_quot;
  logic [34-1:0] div_exe_alu_op1;
  logic [34-1:0] div_exe_alu_op2;
  logic div_exe_alu_add;
  logic div_exe_alu_sub;
  logic [34-1:0] div_exe_alu_res;
  logic [67-1:0] div_exe_part_remd;
  logic [68-1:0] div_exe_part_remd_lsft1;
  logic corrct_phase;
  logic check_phase;
  logic [33-1:0] div_remd;
  logic [33-1:0] div_quot;
  logic remd_is_0;
  logic [34-1:0] div_remd_chck_alu_res_w;
  logic remd_is_neg_divs;
  logic remd_is_divs;
  logic remd_inc_quot_dec;
  logic [34-1:0] div_remd_chck_alu_op1;
  logic [34-1:0] div_remd_chck_alu_op2;
  logic [34-1:0] div_quot_corr_alu_op1;
  logic [34-1:0] div_quot_corr_alu_op2;
  logic div_quot_corr_alu_add;
  logic div_quot_corr_alu_sub;
  logic [34-1:0] div_remd_corr_alu_op1;
  logic [34-1:0] div_remd_corr_alu_op2;
  logic div_remd_corr_alu_add;
  logic div_remd_corr_alu_sub;
  logic [33-1:0] part_prdt_hi_nxt;
  logic [33-1:0] part_prdt_lo_nxt;
  logic [33-1:0] part_remd_nxt;
  logic [33-1:0] part_quot_nxt;
  logic mul_exe_cnt_set;
  logic mul_exe_cnt_inc;
  logic div_exe_cnt_set;
  logic div_exe_cnt_inc;
  logic part_prdt_hi_ena;
  logic part_remd_ena;
  logic part_quot_ena;
  logic req_alu_sel1;
  logic req_alu_sel2;
  logic req_alu_sel3;
  logic req_alu_sel4;
  logic req_alu_sel5;
  logic [32-1:0] mul_res;
  logic [32-1:0] div_res;
  logic [32-1:0] div_special_res;
  logic [32-1:0] back2back_res;
  logic wbck_condi;
  // State register
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      state_r <= 0;
    end else begin
      if (state_0th_exit_ena) begin
        state_r <= 1;
      end else if (state_exec_exit_ena) begin
        state_r <= flush_pulse ? 0 : is_div ? 2 : 0;
      end else if (state_remd_chck_exit_ena) begin
        state_r <= flush_pulse ? 0 : div_need_corrct ? 3 : 0;
      end else if (state_quot_corr_exit_ena) begin
        state_r <= flush_pulse ? 0 : 4;
      end else if (state_remd_corr_exit_ena) begin
        state_r <= 0;
      end
    end
  end
  // Exec counter
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      exec_cnt_r <= 0;
    end else begin
      if (state_exec_enter_ena) begin
        exec_cnt_r <= 1;
      end else if (sta_exec & ~exec_last) begin
        exec_cnt_r <= 6'(exec_cnt_r + 1);
      end else if (state_exec_exit_ena) begin
        exec_cnt_r <= 0;
      end
    end
  end
  // Flushed flag
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      flushed_r <= 1'b0;
    end else begin
      if (flush_pulse) begin
        flushed_r <= 1'b1;
      end else if (o_hsked & ~flush_pulse) begin
        flushed_r <= 1'b0;
      end
    end
  end
  // Part product shift1 register
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      part_prdt_sft1_r <= 1'b0;
    end else begin
      if (is_mul & (state_exec_enter_ena | sta_exec & ~exec_last) | state_exec_exit_ena) begin
        part_prdt_sft1_r <= cycle_0th ? muldiv_i_rs1[1:1] : part_prdt_lo_r[1:1];
      end
    end
  end
  // Part remainder shift1 register
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      part_remd_sft1_r <= 1'b0;
    end else begin
      if (is_div & (state_exec_enter_ena | sta_exec & ~exec_last) | state_exec_exit_ena | state_remd_corr_exit_ena) begin
        part_remd_sft1_r <= muldiv_req_alu_res[32:32];
      end
    end
  end
  assign booth_code = cycle_0th ? {muldiv_i_rs1[1:0], 1'd0} : cycle_16th ? {mul_rs1_sign, part_prdt_lo_r[0:0], part_prdt_sft1_r} : {part_prdt_lo_r[1:0], part_prdt_sft1_r};
  assign booth_sel_zero = booth_code == 0 | booth_code == 7;
  assign booth_sel_two = booth_code == 3 | booth_code == 4;
  assign booth_sel_one = ~booth_sel_zero & ~booth_sel_two;
  assign booth_sel_sub = booth_code[2:2];
  assign mul_exe_alu_op1 = cycle_0th ? 0 : {part_prdt_hi_r[32:32], part_prdt_hi_r[32:32], part_prdt_hi_r};
  assign mul_exe_alu_op2 = (booth_sel_one ? {mul_rs2_sign, mul_rs2_sign, mul_rs2_sign, muldiv_i_rs2} : 0) | (booth_sel_two ? {mul_rs2_sign, mul_rs2_sign, muldiv_i_rs2, 1'd0} : 0);
  assign mul_exe_alu_add = ~booth_sel_sub;
  assign mul_exe_alu_sub = booth_sel_sub;
  assign dividend = {{33{div_rs1_sign}}, div_rs1_sign, muldiv_i_rs1};
  assign divisor = {div_rs2_sign, div_rs2_sign, muldiv_i_rs2};
  assign quot_0cycl = dividend[65:65] ^ divisor[33:33] ? 1'b0 : 1'b1;
  assign dividend_lsft1 = {dividend[65:0], quot_0cycl};
  assign prev_quot = cycle_0th ? quot_0cycl : part_quot_r[0:0];
  assign div_exe_alu_op1 = cycle_0th ? dividend_lsft1[66:33] : {part_remd_sft1_r, part_remd_r[32:0]};
  assign div_exe_alu_op2 = divisor;
  assign div_exe_alu_add = ~prev_quot;
  assign div_exe_alu_sub = prev_quot;
  assign div_exe_alu_res = muldiv_req_alu_res[33:0];
  assign current_quot = div_exe_alu_res[33:33] ^ divisor[33:33] ? 1'b0 : 1'b1;
  assign div_exe_part_remd = {div_exe_alu_res, cycle_0th ? dividend_lsft1[32:0] : part_quot_r[32:0]};
  assign div_exe_part_remd_lsft1 = {div_exe_part_remd[66:0], current_quot};
  assign corrct_phase = sta_remd_corr | sta_quot_corr;
  assign check_phase = sta_remd_chck;
  assign div_remd = check_phase ? part_remd_r : corrct_phase ? muldiv_req_alu_res[32:0] : div_exe_part_remd[65:33];
  assign div_quot = check_phase ? part_quot_r : corrct_phase ? part_quot_r : {div_exe_part_remd[31:0], 1'd1};
  assign remd_is_0 = part_remd_r == 0;
  assign div_remd_chck_alu_res_w = muldiv_req_alu_res[33:0];
  assign remd_is_neg_divs = div_remd_chck_alu_res_w == 0;
  assign remd_is_divs = part_remd_r == divisor[32:0];
  assign div_need_corrct = is_div & ((part_remd_r[32:32] ^ dividend[65:65]) & ~remd_is_0 | remd_is_neg_divs | remd_is_divs);
  assign state_remd_chck_exit_ena = sta_remd_chck & (div_need_corrct | o_hsked | flush_pulse);
  assign remd_inc_quot_dec = part_remd_r[32:32] ^ divisor[33:33];
  assign div_remd_chck_alu_op1 = {part_remd_r[32:32], part_remd_r};
  assign div_remd_chck_alu_op2 = divisor;
  assign div_quot_corr_alu_op1 = {part_quot_r[32:32], part_quot_r};
  assign div_quot_corr_alu_op2 = 1;
  assign div_quot_corr_alu_add = ~remd_inc_quot_dec;
  assign div_quot_corr_alu_sub = remd_inc_quot_dec;
  assign div_remd_corr_alu_op1 = {part_remd_r[32:32], part_remd_r};
  assign div_remd_corr_alu_op2 = divisor;
  assign div_remd_corr_alu_add = remd_inc_quot_dec;
  assign div_remd_corr_alu_sub = ~remd_inc_quot_dec;
  assign part_prdt_hi_nxt = muldiv_req_alu_res[34:2];
  assign part_prdt_lo_nxt = {muldiv_req_alu_res[1:0], cycle_0th ? {mul_rs1_sign, muldiv_i_rs1[31:2]} : part_prdt_lo_r[32:2]};
  assign part_remd_nxt = corrct_phase ? muldiv_req_alu_res[32:0] : sta_exec & cycle_32nd ? div_remd : div_exe_part_remd_lsft1[65:33];
  assign part_quot_nxt = corrct_phase ? muldiv_req_alu_res[32:0] : sta_exec & cycle_32nd ? div_quot : div_exe_part_remd_lsft1[32:0];
  assign mul_exe_cnt_set = state_exec_enter_ena & is_mul;
  assign mul_exe_cnt_inc = sta_exec & ~exec_last & is_mul;
  assign div_exe_cnt_set = state_exec_enter_ena & is_div;
  assign div_exe_cnt_inc = sta_exec & ~exec_last & is_div;
  assign part_prdt_hi_ena = mul_exe_cnt_set | mul_exe_cnt_inc | state_exec_exit_ena;
  assign part_remd_ena = div_exe_cnt_set | div_exe_cnt_inc | state_exec_exit_ena | state_remd_corr_exit_ena;
  assign part_quot_ena = div_exe_cnt_set | div_exe_cnt_inc | state_exec_exit_ena | state_quot_corr_exit_ena;
  assign muldiv_sbf_0_ena = part_remd_ena | part_prdt_hi_ena;
  assign muldiv_sbf_0_nxt = is_mul ? part_prdt_hi_nxt : part_remd_nxt;
  assign muldiv_sbf_1_ena = part_quot_ena | part_prdt_hi_ena;
  assign muldiv_sbf_1_nxt = is_mul ? part_prdt_lo_nxt : part_quot_nxt;
  assign req_alu_sel1 = is_mul;
  assign req_alu_sel2 = is_div & (sta_0th | sta_exec);
  assign req_alu_sel3 = is_div & sta_quot_corr;
  assign req_alu_sel4 = is_div & sta_remd_corr;
  assign req_alu_sel5 = is_div & sta_remd_chck;
  assign muldiv_req_alu_op1 = (req_alu_sel1 ? mul_exe_alu_op1 : 0) | (req_alu_sel2 ? 35'($unsigned(div_exe_alu_op1)) : 0) | (req_alu_sel3 ? 35'($unsigned(div_quot_corr_alu_op1)) : 0) | (req_alu_sel4 ? 35'($unsigned(div_remd_corr_alu_op1)) : 0) | (req_alu_sel5 ? 35'($unsigned(div_remd_chck_alu_op1)) : 0);
  assign muldiv_req_alu_op2 = (req_alu_sel1 ? mul_exe_alu_op2 : 0) | (req_alu_sel2 ? 35'($unsigned(div_exe_alu_op2)) : 0) | (req_alu_sel3 ? 35'($unsigned(div_quot_corr_alu_op2)) : 0) | (req_alu_sel4 ? 35'($unsigned(div_remd_corr_alu_op2)) : 0) | (req_alu_sel5 ? 35'($unsigned(div_remd_chck_alu_op2)) : 0);
  assign muldiv_req_alu_add = req_alu_sel1 & mul_exe_alu_add | req_alu_sel2 & div_exe_alu_add | req_alu_sel3 & div_quot_corr_alu_add | req_alu_sel4 & div_remd_corr_alu_add | req_alu_sel5;
  assign muldiv_req_alu_sub = req_alu_sel1 & mul_exe_alu_sub | req_alu_sel2 & div_exe_alu_sub | req_alu_sel3 & div_quot_corr_alu_sub | req_alu_sel4 & div_remd_corr_alu_sub;
  assign mul_res = i_mul ? part_prdt_lo_r[32:1] : muldiv_req_alu_res[31:0];
  assign div_res = i_div | i_divu ? div_quot[31:0] : div_remd[31:0];
  assign div_special_res = div_by_0 ? i_div | i_divu ? 32'd4294967295 : muldiv_i_rs1 : i_div | i_divu ? 32'd2147483648 : 0;
  assign back2back_res = (i_mul ? {part_prdt_lo_r[30:0], part_prdt_sft1_r} : 0) | (i_rem | i_remu ? part_remd_r[31:0] : 0) | (i_div | i_divu ? part_quot_r[31:0] : 0);
  assign wbck_condi = back2back_seq | special_cases ? 1'b1 : sta_exec & exec_last & ~is_div | sta_remd_chck & ~div_need_corrct | sta_remd_corr;
  assign muldiv_o_valid = wbck_condi & muldiv_i_valid;
  assign muldiv_i_ready = wbck_condi & muldiv_o_ready;
  assign muldiv_o_wbck_wdat = (back2back_seq & ~special_cases ? back2back_res : 0) | (special_cases ? div_special_res : 0) | (~back2back_seq & ~special_cases & is_div ? div_res : 0) | (~back2back_seq & ~special_cases & is_mul ? mul_res : 0);
  assign muldiv_o_wbck_err = 1'b0;
  assign muldiv_i_longpipe = 1'b0;

endmodule

// Booth multiply
// Divide
// Correction check
// Correction ALU operands
// Buffer next values
// Buffer enables
// ALU operand muxing
// Results
// Special results
// Back-to-back results
// Output
