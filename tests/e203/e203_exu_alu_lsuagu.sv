// E203 Load/Store Address Generation Unit
// Generates addresses for load/store/AMO instructions.
// AMO uses a multi-step state machine: read -> ALU compute -> write -> writeback.
module e203_exu_alu_lsuagu (
  input logic clk,
  input logic rst_n,
  input logic agu_i_valid,
  output logic agu_i_ready,
  input logic [32-1:0] agu_i_rs1,
  input logic [32-1:0] agu_i_rs2,
  input logic [32-1:0] agu_i_imm,
  input logic [21-1:0] agu_i_info,
  input logic [1-1:0] agu_i_itag,
  output logic agu_i_longpipe,
  input logic flush_req,
  input logic flush_pulse,
  output logic amo_wait,
  input logic oitf_empty,
  output logic agu_o_valid,
  input logic agu_o_ready,
  output logic [32-1:0] agu_o_wbck_wdat,
  output logic agu_o_wbck_err,
  output logic agu_o_cmt_misalgn,
  output logic agu_o_cmt_ld,
  output logic agu_o_cmt_stamo,
  output logic agu_o_cmt_buserr,
  output logic [32-1:0] agu_o_cmt_badaddr,
  output logic agu_icb_cmd_valid,
  input logic agu_icb_cmd_ready,
  output logic [32-1:0] agu_icb_cmd_addr,
  output logic agu_icb_cmd_read,
  output logic [32-1:0] agu_icb_cmd_wdata,
  output logic [4-1:0] agu_icb_cmd_wmask,
  output logic agu_icb_cmd_back2agu,
  output logic agu_icb_cmd_lock,
  output logic agu_icb_cmd_excl,
  output logic [2-1:0] agu_icb_cmd_size,
  output logic [1-1:0] agu_icb_cmd_itag,
  output logic agu_icb_cmd_usign,
  input logic agu_icb_rsp_valid,
  output logic agu_icb_rsp_ready,
  input logic agu_icb_rsp_err,
  input logic agu_icb_rsp_excl_ok,
  input logic [32-1:0] agu_icb_rsp_rdata,
  output logic [32-1:0] agu_req_alu_op1,
  output logic [32-1:0] agu_req_alu_op2,
  output logic agu_req_alu_swap,
  output logic agu_req_alu_add,
  output logic agu_req_alu_and,
  output logic agu_req_alu_or,
  output logic agu_req_alu_xor,
  output logic agu_req_alu_max,
  output logic agu_req_alu_min,
  output logic agu_req_alu_maxu,
  output logic agu_req_alu_minu,
  input logic [32-1:0] agu_req_alu_res,
  output logic agu_sbf_0_ena,
  output logic [32-1:0] agu_sbf_0_nxt,
  input logic [32-1:0] agu_sbf_0_r,
  output logic agu_sbf_1_ena,
  output logic [32-1:0] agu_sbf_1_nxt,
  input logic [32-1:0] agu_sbf_1_r
);

  // Dispatch handshake
  // Flush
  // AMO state
  // Result handshake
  // ICB command interface
  // ICB response interface
  // Shared ALU datapath
  // Shared buffers
  // Decode info fields (from agu_i_info)
  logic i_load;
  assign i_load = agu_i_info[4:4];
  logic i_store;
  assign i_store = agu_i_info[5:5];
  logic [2-1:0] i_size;
  assign i_size = agu_i_info[7:6];
  logic i_usign;
  assign i_usign = agu_i_info[8:8];
  logic i_excl;
  assign i_excl = agu_i_info[9:9];
  logic i_amo;
  assign i_amo = agu_i_info[10:10];
  logic i_amoswap;
  assign i_amoswap = agu_i_info[11:11];
  logic i_amoadd;
  assign i_amoadd = agu_i_info[12:12];
  logic i_amoand;
  assign i_amoand = agu_i_info[13:13];
  logic i_amoor;
  assign i_amoor = agu_i_info[14:14];
  logic i_amoxor;
  assign i_amoxor = agu_i_info[15:15];
  logic i_amomax;
  assign i_amomax = agu_i_info[16:16];
  logic i_amomin;
  assign i_amomin = agu_i_info[17:17];
  logic i_amomaxu;
  assign i_amomaxu = agu_i_info[18:18];
  logic i_amominu;
  assign i_amominu = agu_i_info[19:19];
  logic size_b;
  assign size_b = i_size == 0;
  logic size_hw;
  assign size_hw = i_size == 1;
  logic size_w;
  assign size_w = i_size == 2;
  // AMO ICB state machine
  // IDLE=0, 1ST=1, WAIT2ND=2, 2ND=3, AMOALU=4, AMORDY=5, WBCK=6
  logic [4-1:0] icb_state_r;
  logic sta_idle;
  assign sta_idle = icb_state_r == 0;
  logic sta_1st;
  assign sta_1st = icb_state_r == 1;
  logic sta_wait2nd;
  assign sta_wait2nd = icb_state_r == 2;
  logic sta_2nd;
  assign sta_2nd = icb_state_r == 3;
  logic sta_amoalu;
  assign sta_amoalu = icb_state_r == 4;
  logic sta_amordy;
  assign sta_amordy = icb_state_r == 5;
  logic sta_wbck;
  assign sta_wbck = icb_state_r == 6;
  logic flush_block;
  assign flush_block = flush_req & sta_idle;
  logic ld;
  assign ld = i_load & ~flush_block;
  logic st;
  assign st = i_store & ~flush_block;
  logic amo;
  assign amo = i_amo & ~flush_block;
  logic ofst0;
  assign ofst0 = amo | (ld | st) & i_excl;
  // Address alignment check (uses agu_req_alu_res = computed address)
  logic addr_unalgn;
  logic algnld;
  logic algnst;
  logic algn_ldst;
  logic algn_amo;
  logic unalgn_ldst;
  logic unalgn_amo;
  // Store data/mask
  logic [32-1:0] algnst_wdata;
  logic [4-1:0] algnst_wmask;
  // Address generation offset
  logic [32-1:0] addr_gen_op2;
  assign addr_gen_op2 = ofst0 ? 0 : agu_i_imm;
  // ICB handshake signals
  logic icb_cmd_hsked;
  assign icb_cmd_hsked = agu_icb_cmd_valid & agu_icb_cmd_ready;
  logic icb_rsp_hsked;
  assign icb_rsp_hsked = agu_icb_rsp_valid & agu_icb_rsp_ready;
  // AMO uop flags
  logic amo_1stuop;
  assign amo_1stuop = sta_1st & algn_amo;
  logic amo_2nduop;
  assign amo_2nduop = sta_2nd & algn_amo;
  // Leftover buffer (shared with sbf_0)
  logic leftover_ena;
  logic [32-1:0] leftover_nxt;
  logic [32-1:0] leftover_r;
  assign leftover_r = agu_sbf_0_r;
  // Leftover error tracking
  logic leftover_err_ena;
  logic leftover_err_nxt;
  logic leftover_err_r;
  // Leftover_1 buffer (shared with sbf_1) for ALU result
  logic leftover_1_ena;
  logic [32-1:0] leftover_1_nxt;
  logic [32-1:0] leftover_1_r;
  assign leftover_1_r = agu_sbf_1_r;
  // State machine exit enable signals
  logic state_idle_exit_ena;
  assign state_idle_exit_ena = sta_idle & algn_amo & oitf_empty & icb_cmd_hsked & ~flush_pulse;
  logic state_1st_exit_ena;
  assign state_1st_exit_ena = sta_1st & (icb_rsp_hsked | flush_pulse);
  logic state_amoalu_exit_ena;
  assign state_amoalu_exit_ena = sta_amoalu;
  logic state_amordy_exit_ena;
  assign state_amordy_exit_ena = sta_amordy;
  logic state_wait2nd_exit_ena;
  assign state_wait2nd_exit_ena = sta_wait2nd & (agu_icb_cmd_ready | flush_pulse);
  logic state_2nd_exit_ena;
  assign state_2nd_exit_ena = sta_2nd & (icb_rsp_hsked | flush_pulse);
  logic state_wbck_exit_ena;
  assign state_wbck_exit_ena = sta_wbck & (agu_o_ready | flush_pulse);
  logic state_last_exit_ena;
  assign state_last_exit_ena = state_wbck_exit_ena;
  // State machine update
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      icb_state_r <= 0;
    end else begin
      if (state_idle_exit_ena) begin
        icb_state_r <= 1;
      end else if (state_1st_exit_ena) begin
        icb_state_r <= flush_pulse ? 0 : 4;
      end else if (state_amoalu_exit_ena) begin
        icb_state_r <= flush_pulse ? 0 : 5;
      end else if (state_amordy_exit_ena) begin
        icb_state_r <= flush_pulse ? 0 : 2;
      end else if (state_wait2nd_exit_ena) begin
        icb_state_r <= flush_pulse ? 0 : 3;
      end else if (state_2nd_exit_ena) begin
        icb_state_r <= flush_pulse ? 0 : 6;
      end else if (state_wbck_exit_ena) begin
        icb_state_r <= 0;
      end
    end
  end
  // Leftover error register
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      leftover_err_r <= 1'b0;
    end else begin
      if (leftover_err_ena) begin
        leftover_err_r <= leftover_err_nxt;
      end
    end
  end
  assign addr_unalgn = size_hw & agu_icb_cmd_addr[0:0] | size_w & agu_icb_cmd_addr[1:0] != 0;
  assign algnld = ~addr_unalgn & ld & ~amo;
  assign algnst = ~addr_unalgn & st & ~amo;
  assign algn_ldst = algnld | algnst;
  assign algn_amo = ~addr_unalgn & amo;
  assign unalgn_ldst = addr_unalgn & (ld | st) & ~amo;
  assign unalgn_amo = addr_unalgn & amo;
  assign algnst_wdata = size_b ? {4{agu_i_rs2[7:0]}} : size_hw ? {2{agu_i_rs2[15:0]}} : agu_i_rs2;
  assign algnst_wmask = size_b ? 4'd1 << agu_icb_cmd_addr[1:0] : size_hw ? 4'd3 << {agu_icb_cmd_addr[1:1], 1'd0} : 4'd15;
  assign agu_req_alu_op1 = sta_idle ? agu_i_rs1 : sta_amoalu ? leftover_r : i_amo & (sta_wait2nd | sta_2nd | sta_wbck) ? agu_i_rs1 : 0;
  assign agu_req_alu_op2 = sta_idle ? addr_gen_op2 : sta_amoalu ? agu_i_rs2 : i_amo & (sta_wait2nd | sta_2nd | sta_wbck) ? addr_gen_op2 : 0;
  assign agu_req_alu_add = sta_amoalu & i_amoadd | i_amo & (sta_wait2nd | sta_2nd | sta_wbck) | sta_idle;
  assign agu_req_alu_swap = sta_amoalu & i_amoswap;
  assign agu_req_alu_and = sta_amoalu & i_amoand;
  assign agu_req_alu_or = sta_amoalu & i_amoor;
  assign agu_req_alu_xor = sta_amoalu & i_amoxor;
  assign agu_req_alu_max = sta_amoalu & i_amomax;
  assign agu_req_alu_min = sta_amoalu & i_amomin;
  assign agu_req_alu_maxu = sta_amoalu & i_amomaxu;
  assign agu_req_alu_minu = sta_amoalu & i_amominu;
  assign leftover_ena = icb_rsp_hsked & (amo_1stuop | amo_2nduop);
  assign leftover_nxt = amo_1stuop ? agu_icb_rsp_rdata : leftover_r;
  assign leftover_err_ena = leftover_ena;
  assign leftover_err_nxt = amo_1stuop & agu_icb_rsp_err | amo_2nduop & (agu_icb_rsp_err | leftover_err_r);
  assign agu_sbf_0_ena = leftover_ena;
  assign agu_sbf_0_nxt = leftover_nxt;
  assign leftover_1_ena = sta_amoalu;
  assign leftover_1_nxt = agu_req_alu_res;
  assign agu_sbf_1_ena = leftover_1_ena;
  assign agu_sbf_1_nxt = leftover_1_nxt;
  assign agu_icb_cmd_valid = algn_ldst & agu_i_valid & agu_o_ready | algn_amo & (sta_idle & agu_i_valid & agu_o_ready | sta_wait2nd) | unalgn_amo & 1'b0;
  assign agu_icb_cmd_addr = agu_req_alu_res;
  assign agu_icb_cmd_read = algn_ldst & ld | algn_amo & sta_idle;
  assign agu_icb_cmd_wdata = amo ? leftover_1_r : algnst_wdata;
  assign agu_icb_cmd_wmask = amo ? leftover_err_r ? 0 : 4'd15 : algnst_wmask;
  assign agu_icb_cmd_back2agu = algn_amo;
  assign agu_icb_cmd_lock = algn_amo & sta_idle;
  assign agu_icb_cmd_excl = i_excl;
  assign agu_icb_cmd_size = i_size;
  assign agu_icb_cmd_itag = agu_i_itag;
  assign agu_icb_cmd_usign = i_usign;
  assign agu_icb_rsp_ready = 1'b1;
  assign agu_o_valid = sta_wbck | agu_i_valid & (algn_ldst | unalgn_ldst | unalgn_amo) & agu_icb_cmd_ready;
  assign agu_o_wbck_wdat = algn_amo ? leftover_r : 0;
  assign agu_o_cmt_misalgn = unalgn_amo | unalgn_ldst;
  assign agu_o_cmt_ld = ld & ~i_excl;
  assign agu_o_cmt_stamo = st | amo | i_excl;
  assign agu_o_cmt_buserr = algn_amo & leftover_err_r;
  assign agu_o_cmt_badaddr = agu_icb_cmd_addr;
  assign agu_o_wbck_err = agu_o_cmt_buserr | agu_o_cmt_misalgn;
  assign agu_i_ready = algn_amo ? state_last_exit_ena : agu_icb_cmd_ready & agu_o_ready;
  assign agu_i_longpipe = algn_ldst;
  assign amo_wait = ~sta_idle;

endmodule

// Address alignment
// Store data: byte/half replicated
// Store mask: shifted based on address LSBs
// ALU operand 1
// ALU operand 2
// ALU operation selection
// Leftover buffer 0: loaded on response handshake during AMO 1st or 2nd uop
// Leftover error: merge errors from both uops
// Leftover buffer 1: ALU result saved during AMOALU
// ICB command valid
// Output valid: AMO wbck state OR (normal ldst/unalgn at dispatch with cmd_ready)
// Output data
// Commit signals (use flush-blocked versions matching reference)
// Ready: AMO goes through state machine, others go directly
