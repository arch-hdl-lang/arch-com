// domain SysDomain
//   freq_mhz: 100

// domain SysDomain
//   freq_mhz: 100

// domain SysDomain
//   freq_mhz: 100

// domain SysDomain
//   freq_mhz: 100

// domain SysDomain
//   freq_mhz: 100

// domain SysDomain
//   freq_mhz: 100

// domain SysDomain
//   freq_mhz: 100

module e203_exu_top #(
  parameter int XLEN = 32
) (
  input logic clk,
  input logic rst_n,
  input logic i_valid,
  output logic i_ready,
  input logic [31:0] i_ir,
  input logic [31:0] i_pc,
  input logic i_pc_vld,
  input logic i_misalgn,
  input logic i_buserr,
  input logic i_prdt_taken,
  input logic i_muldiv_b2b,
  input logic [4:0] i_rs1idx,
  input logic [4:0] i_rs2idx,
  output logic pipe_flush_req,
  input logic pipe_flush_ack,
  output logic [31:0] pipe_flush_add_op1,
  output logic [31:0] pipe_flush_add_op2,
  output logic [31:0] pipe_flush_pc,
  input logic lsu_o_valid,
  output logic lsu_o_ready,
  input logic [31:0] lsu_o_wbck_wdat,
  input logic [4:0] lsu_o_wbck_itag,
  input logic lsu_o_wbck_err,
  input logic lsu_o_cmt_ld,
  input logic lsu_o_cmt_st,
  input logic [31:0] lsu_o_cmt_badaddr,
  input logic lsu_o_cmt_buserr,
  output logic agu_icb_cmd_valid,
  input logic agu_icb_cmd_ready,
  output logic [31:0] agu_icb_cmd_addr,
  output logic agu_icb_cmd_read,
  output logic [31:0] agu_icb_cmd_wdata,
  output logic [3:0] agu_icb_cmd_wmask,
  output logic agu_icb_cmd_lock,
  output logic agu_icb_cmd_excl,
  output logic [1:0] agu_icb_cmd_size,
  output logic agu_icb_cmd_back2agu,
  output logic agu_icb_cmd_usign,
  output logic agu_icb_cmd_itag,
  input logic agu_icb_rsp_valid,
  output logic agu_icb_rsp_ready,
  input logic [31:0] agu_icb_rsp_rdata,
  input logic agu_icb_rsp_err,
  input logic agu_icb_rsp_excl_ok,
  input logic dbg_mode,
  input logic dbg_halt_r,
  input logic dbg_step_r,
  input logic dbg_ebreakm_r,
  input logic dbg_stopcycle,
  input logic dbg_irq_r,
  input logic lcl_irq_r,
  input logic evt_r,
  input logic ext_irq_r,
  input logic sft_irq_r,
  input logic tmr_irq_r,
  output logic [31:0] cmt_dpc,
  output logic cmt_dpc_ena,
  output logic [2:0] cmt_dcause,
  output logic cmt_dcause_ena,
  output logic wr_dcsr_ena,
  output logic wr_dpc_ena,
  output logic wr_dscratch_ena,
  output logic [31:0] wr_csr_nxt,
  input logic [31:0] dcsr_r,
  input logic [31:0] dpc_r,
  input logic [31:0] dscratch_r,
  output logic wfi_halt_ifu_req,
  input logic wfi_halt_ifu_ack,
  output logic core_wfi,
  output logic [31:0] rf2ifu_x1,
  output logic [31:0] rf2ifu_rs1,
  output logic dec2ifu_rden,
  output logic dec2ifu_rs1en,
  output logic [4:0] dec2ifu_rdidx,
  output logic dec2ifu_mulhsu,
  output logic dec2ifu_div,
  output logic dec2ifu_rem,
  output logic dec2ifu_divu,
  output logic dec2ifu_remu,
  output logic oitf_empty,
  output logic exu_active,
  output logic excp_active,
  output logic commit_mret,
  output logic commit_trap,
  input logic [31:0] core_mhartid,
  output logic tm_stop,
  output logic itcm_nohold,
  output logic core_cgstop,
  output logic tcm_cgstop,
  output logic nice_req_valid,
  input logic nice_req_ready,
  output logic [31:0] nice_req_inst,
  output logic [31:0] nice_req_rs1,
  output logic [31:0] nice_req_rs2,
  input logic nice_rsp_multicyc_valid,
  output logic nice_rsp_multicyc_ready,
  input logic [31:0] nice_rsp_multicyc_dat,
  input logic nice_rsp_multicyc_err,
  input logic test_mode,
  input logic clk_aon
);

  logic [4:0] dec_rs1_idx;
  logic [4:0] dec_rs2_idx;
  logic [4:0] dec_rd_idx;
  logic [31:0] dec_imm;
  logic dec_bjp;
  logic dec_mul;
  logic dec_mulhsu;
  logic dec_div;
  logic dec_divu;
  logic dec_rem;
  logic dec_remu;
  logic dec_rs1_en;
  logic dec_rs2_en;
  logic dec_rd_en;
  logic dec_rs1x0;
  logic dec_rs2x0;
  logic [31:0] dec_info;
  logic [31:0] dec_pc_out;
  logic dec_misalgn;
  logic dec_buserr_out;
  logic dec_ilegl;
  logic dec_nice;
  logic dec_nice_cmt_off_ilgl;
  logic dec_rv32;
  logic dec_jal;
  logic dec_jalr;
  logic dec_bxx;
  logic [4:0] dec_jalr_rs1idx;
  logic [31:0] dec_bjp_imm;
  logic [31:0] rf_rs1_data;
  logic [31:0] rf_rs2_data;
  logic disp_rdy;
  logic disp_wfi_halt_exu_ack;
  logic disp_alu_valid;
  logic [31:0] disp_alu_rs1;
  logic [31:0] disp_alu_rs2;
  logic [31:0] disp_alu_pc;
  logic [31:0] disp_alu_imm;
  logic [4:0] disp_alu_rdidx;
  logic disp_alu_rdwen;
  logic [31:0] disp_alu_info;
  logic [0:0] disp_alu_itag;
  logic disp_alu_misalgn;
  logic disp_alu_buserr;
  logic disp_alu_ilegl;
  logic disp_oitf_rs1fpu_w;
  logic disp_oitf_rs2fpu_w;
  logic disp_oitf_rs3fpu_w;
  logic disp_oitf_rdfpu_w;
  logic disp_oitf_rs1en_w;
  logic disp_oitf_rs2en_w;
  logic disp_oitf_rs3en_w;
  logic disp_oitf_rdwen_w;
  logic [4:0] disp_oitf_rs1idx_w;
  logic [4:0] disp_oitf_rs2idx_w;
  logic [4:0] disp_oitf_rs3idx_w;
  logic [4:0] disp_oitf_rdidx_w;
  logic [31:0] disp_oitf_pc_w;
  logic oitf_dis_ready;
  logic [0:0] oitf_dis_ptr;
  logic [0:0] oitf_ret_ptr;
  logic [4:0] oitf_ret_rdidx;
  logic oitf_ret_rdwen;
  logic oitf_ret_rdfpu;
  logic [31:0] oitf_ret_pc;
  logic oitf_match_rs1;
  logic oitf_match_rs2;
  logic oitf_match_rs3;
  logic oitf_match_rd;
  logic oitf_is_empty;
  logic alu_o_ready;
  logic alu_longpipe;
  logic [31:0] alu_wdat;
  logic [4:0] alu_rdidx;
  logic alu_wbck_valid;
  logic alu_cmt_valid;
  logic alu_cmt_pc_vld;
  logic [31:0] alu_cmt_pc;
  logic [31:0] alu_cmt_instr;
  logic [31:0] alu_cmt_imm;
  logic alu_cmt_rv32;
  logic alu_cmt_bjp;
  logic alu_cmt_mret;
  logic alu_cmt_dret;
  logic alu_cmt_ecall;
  logic alu_cmt_ebreak;
  logic alu_cmt_fencei;
  logic alu_cmt_wfi;
  logic alu_cmt_ifu_misalgn;
  logic alu_cmt_ifu_buserr;
  logic alu_cmt_ifu_ilegl;
  logic alu_cmt_bjp_prdt;
  logic alu_cmt_bjp_rslv;
  logic alu_cmt_misalgn;
  logic alu_cmt_ld;
  logic alu_cmt_stamo;
  logic alu_cmt_buserr;
  logic [31:0] alu_cmt_badaddr;
  logic alu_csr_ena;
  logic alu_csr_wr_en;
  logic alu_csr_rd_en;
  logic [11:0] alu_csr_idx;
  logic [31:0] alu_wbck_csr_dat;
  logic alu_nice_longp_wbck_valid;
  logic alu_nice_longp_wbck_ready;
  logic [0:0] alu_nice_o_itag;
  logic amo_wait_w;
  logic oitf_empty_w;
  logic pipe_flush_pulse_w;
  logic [31:0] read_csr_dat_w;
  logic alu_done_valid;
  logic longp_wbck_valid;
  logic [31:0] longp_wbck_wdat;
  logic [4:0] longp_wbck_rdidx;
  logic [4:0] longp_wbck_flags;
  logic longp_wbck_rdfpu;
  logic longp_lsu_ready;
  logic longp_nice_ready;
  logic longp_excp_valid;
  logic longp_excp_insterr;
  logic longp_excp_ld;
  logic longp_excp_st;
  logic longp_excp_buserr;
  logic [31:0] longp_excp_badaddr;
  logic [31:0] longp_excp_pc;
  logic longp_excp_ready_from_commit;
  logic wbck_alu_ready;
  logic wbck_longp_ready;
  logic wbck_rf_ena;
  logic [31:0] wbck_rf_wdat;
  logic [4:0] wbck_rf_rdidx;
  logic commit_alu_ready;
  logic commit_mret_w;
  logic commit_trap_w;
  logic core_wfi_w;
  logic nonflush_cmt_ena_w;
  logic excp_active_w;
  logic wfi_halt_ifu_req_w;
  logic wfi_halt_exu_req_w;
  logic [31:0] cmt_badaddr_w;
  logic cmt_badaddr_ena_w;
  logic [31:0] cmt_epc_w;
  logic cmt_epc_ena_w;
  logic [31:0] cmt_cause_w;
  logic cmt_cause_ena_w;
  logic cmt_instret_ena_w;
  logic cmt_status_ena_w;
  logic [31:0] cmt_dpc_w;
  logic cmt_dpc_ena_w;
  logic [2:0] cmt_dcause_w;
  logic cmt_dcause_ena_w;
  logic cmt_mret_ena_w;
  logic flush_pulse_w;
  logic flush_req_w;
  logic pipe_flush_req_w;
  logic [31:0] pipe_flush_add_op1_w;
  logic [31:0] pipe_flush_add_op2_w;
  logic [31:0] pipe_flush_pc_w;
  logic [31:0] csr_rdata;
  logic [31:0] csr_mtvec_val;
  logic [31:0] csr_mepc_val;
  logic [31:0] csr_dpc_val;
  logic csr_access_ilgl_w;
  logic csr_nice_xs_off;
  logic csr_tm_stop;
  logic csr_core_cgstop;
  logic csr_tcm_cgstop;
  logic csr_itcm_nohold;
  logic csr_mdv_nob2b;
  logic csr_status_mie;
  logic csr_mtie;
  logic csr_msie;
  logic csr_meie;
  logic csr_wr_dcsr_ena;
  logic csr_wr_dpc_ena;
  logic csr_wr_dscratch_ena;
  logic [31:0] csr_wr_csr_nxt;
  logic csr_u_mode;
  logic csr_s_mode;
  logic csr_h_mode;
  logic csr_m_mode;
  logic [31:0] rf_x1_r;
  logic disp_valid_gated;
  logic oitf_dis_ena;
  logic oitf_ret_ena;
  e203_exu_decode dec (
    .i_instr(i_ir),
    .i_pc(i_pc),
    .i_prdt_taken(i_prdt_taken),
    .i_misalgn(i_misalgn),
    .i_buserr(i_buserr),
    .i_muldiv_b2b(i_muldiv_b2b),
    .dbg_mode(dbg_mode),
    .nice_xs_off(0),
    .dec_rs1idx(dec_rs1_idx),
    .dec_rs2idx(dec_rs2_idx),
    .dec_rdidx(dec_rd_idx),
    .dec_imm(dec_imm),
    .dec_bjp(dec_bjp),
    .dec_rs1en(dec_rs1_en),
    .dec_rs2en(dec_rs2_en),
    .dec_rdwen(dec_rd_en),
    .dec_mul(dec_mul),
    .dec_mulhsu(dec_mulhsu),
    .dec_div(dec_div),
    .dec_divu(dec_divu),
    .dec_rem(dec_rem),
    .dec_remu(dec_remu),
    .dec_rs1x0(dec_rs1x0),
    .dec_rs2x0(dec_rs2x0),
    .dec_info(dec_info),
    .dec_pc(dec_pc_out),
    .dec_misalgn(dec_misalgn),
    .dec_buserr(dec_buserr_out),
    .dec_ilegl(dec_ilegl),
    .dec_nice(dec_nice),
    .nice_cmt_off_ilgl_o(dec_nice_cmt_off_ilgl),
    .dec_rv32(dec_rv32),
    .dec_jal(dec_jal),
    .dec_jalr(dec_jalr),
    .dec_bxx(dec_bxx),
    .dec_jalr_rs1idx(dec_jalr_rs1idx),
    .dec_bjp_imm(dec_bjp_imm)
  );
  e203_exu_disp disp (
    .clk(clk),
    .rst_n(rst_n),
    .wfi_halt_exu_req(wfi_halt_exu_req_w),
    .wfi_halt_exu_ack(disp_wfi_halt_exu_ack),
    .oitf_empty(oitf_is_empty),
    .amo_wait(amo_wait_w),
    .disp_i_valid(disp_valid_gated),
    .disp_i_ready(disp_rdy),
    .disp_i_rs1x0(dec_rs1x0),
    .disp_i_rs2x0(dec_rs2x0),
    .disp_i_rs1en(dec_rs1_en),
    .disp_i_rs2en(dec_rs2_en),
    .disp_i_rs1idx(i_rs1idx),
    .disp_i_rs2idx(i_rs2idx),
    .disp_i_rs1(rf_rs1_data),
    .disp_i_rs2(rf_rs2_data),
    .disp_i_rdwen(dec_rd_en),
    .disp_i_rdidx(dec_rd_idx),
    .disp_i_info(dec_info),
    .disp_i_imm(dec_imm),
    .disp_i_pc(dec_pc_out),
    .disp_i_misalgn(dec_misalgn),
    .disp_i_buserr(dec_buserr_out),
    .disp_i_ilegl(dec_ilegl),
    .disp_o_alu_valid(disp_alu_valid),
    .disp_o_alu_ready(alu_o_ready),
    .disp_o_alu_longpipe(alu_longpipe),
    .disp_o_alu_rs1(disp_alu_rs1),
    .disp_o_alu_rs2(disp_alu_rs2),
    .disp_o_alu_rdwen(disp_alu_rdwen),
    .disp_o_alu_rdidx(disp_alu_rdidx),
    .disp_o_alu_info(disp_alu_info),
    .disp_o_alu_imm(disp_alu_imm),
    .disp_o_alu_pc(disp_alu_pc),
    .disp_o_alu_itag(disp_alu_itag),
    .disp_o_alu_misalgn(disp_alu_misalgn),
    .disp_o_alu_buserr(disp_alu_buserr),
    .disp_o_alu_ilegl(disp_alu_ilegl),
    .oitfrd_match_disprs1(oitf_match_rs1),
    .oitfrd_match_disprs2(oitf_match_rs2),
    .oitfrd_match_disprs3(oitf_match_rs3),
    .oitfrd_match_disprd(oitf_match_rd),
    .disp_oitf_ptr(oitf_dis_ptr),
    .disp_oitf_ena(oitf_dis_ena),
    .disp_oitf_ready(oitf_dis_ready),
    .disp_oitf_rs1fpu(disp_oitf_rs1fpu_w),
    .disp_oitf_rs2fpu(disp_oitf_rs2fpu_w),
    .disp_oitf_rs3fpu(disp_oitf_rs3fpu_w),
    .disp_oitf_rdfpu(disp_oitf_rdfpu_w),
    .disp_oitf_rs1en(disp_oitf_rs1en_w),
    .disp_oitf_rs2en(disp_oitf_rs2en_w),
    .disp_oitf_rs3en(disp_oitf_rs3en_w),
    .disp_oitf_rdwen(disp_oitf_rdwen_w),
    .disp_oitf_rs1idx(disp_oitf_rs1idx_w),
    .disp_oitf_rs2idx(disp_oitf_rs2idx_w),
    .disp_oitf_rs3idx(disp_oitf_rs3idx_w),
    .disp_oitf_rdidx(disp_oitf_rdidx_w),
    .disp_oitf_pc(disp_oitf_pc_w)
  );
  e203_exu_oitf oitf_u (
    .clk(clk),
    .rst_n(rst_n),
    .dis_ready(oitf_dis_ready),
    .dis_ena(oitf_dis_ena),
    .ret_ena(oitf_ret_ena),
    .dis_ptr(oitf_dis_ptr),
    .ret_ptr(oitf_ret_ptr),
    .ret_rdidx(oitf_ret_rdidx),
    .ret_rdwen(oitf_ret_rdwen),
    .ret_rdfpu(oitf_ret_rdfpu),
    .ret_pc(oitf_ret_pc),
    .disp_i_rs1en(disp_oitf_rs1en_w),
    .disp_i_rs2en(disp_oitf_rs2en_w),
    .disp_i_rs3en(disp_oitf_rs3en_w),
    .disp_i_rdwen(disp_oitf_rdwen_w),
    .disp_i_rs1fpu(disp_oitf_rs1fpu_w),
    .disp_i_rs2fpu(disp_oitf_rs2fpu_w),
    .disp_i_rs3fpu(disp_oitf_rs3fpu_w),
    .disp_i_rdfpu(disp_oitf_rdfpu_w),
    .disp_i_rs1idx(disp_oitf_rs1idx_w),
    .disp_i_rs2idx(disp_oitf_rs2idx_w),
    .disp_i_rs3idx(disp_oitf_rs3idx_w),
    .disp_i_rdidx(disp_oitf_rdidx_w),
    .disp_i_pc(disp_oitf_pc_w),
    .oitfrd_match_disprs1(oitf_match_rs1),
    .oitfrd_match_disprs2(oitf_match_rs2),
    .oitfrd_match_disprs3(oitf_match_rs3),
    .oitfrd_match_disprd(oitf_match_rd),
    .oitf_empty(oitf_is_empty)
  );
  e203_exu_alu alu_u (
    .clk(clk),
    .rst_n(rst_n),
    .i_valid(disp_alu_valid),
    .i_ready(alu_o_ready),
    .i_longpipe(alu_longpipe),
    .i_rs1(disp_alu_rs1),
    .i_rs2(disp_alu_rs2),
    .i_imm(disp_alu_imm),
    .i_info(disp_alu_info),
    .i_pc(disp_alu_pc),
    .i_instr(i_ir),
    .i_pc_vld(i_pc_vld),
    .i_rdidx(disp_alu_rdidx),
    .i_rdwen(disp_alu_rdwen),
    .i_itag(disp_alu_itag),
    .i_ilegl(disp_alu_ilegl),
    .i_buserr(disp_alu_buserr),
    .i_misalgn(disp_alu_misalgn),
    .nice_xs_off(csr_nice_xs_off),
    .amo_wait(amo_wait_w),
    .oitf_empty(oitf_empty_w),
    .flush_req(flush_req_w),
    .flush_pulse(pipe_flush_pulse_w),
    .mdv_nob2b(csr_mdv_nob2b),
    .i_nice_cmt_off_ilgl(1'b0),
    .cmt_o_valid(alu_cmt_valid),
    .cmt_o_ready(commit_alu_ready),
    .cmt_o_pc_vld(alu_cmt_pc_vld),
    .cmt_o_pc(alu_cmt_pc),
    .cmt_o_instr(alu_cmt_instr),
    .cmt_o_imm(alu_cmt_imm),
    .cmt_o_rv32(alu_cmt_rv32),
    .cmt_o_bjp(alu_cmt_bjp),
    .cmt_o_mret(alu_cmt_mret),
    .cmt_o_dret(alu_cmt_dret),
    .cmt_o_ecall(alu_cmt_ecall),
    .cmt_o_ebreak(alu_cmt_ebreak),
    .cmt_o_fencei(alu_cmt_fencei),
    .cmt_o_wfi(alu_cmt_wfi),
    .cmt_o_ifu_misalgn(alu_cmt_ifu_misalgn),
    .cmt_o_ifu_buserr(alu_cmt_ifu_buserr),
    .cmt_o_ifu_ilegl(alu_cmt_ifu_ilegl),
    .cmt_o_bjp_prdt(alu_cmt_bjp_prdt),
    .cmt_o_bjp_rslv(alu_cmt_bjp_rslv),
    .cmt_o_misalgn(alu_cmt_misalgn),
    .cmt_o_ld(alu_cmt_ld),
    .cmt_o_stamo(alu_cmt_stamo),
    .cmt_o_buserr(alu_cmt_buserr),
    .cmt_o_badaddr(alu_cmt_badaddr),
    .wbck_o_valid(alu_wbck_valid),
    .wbck_o_ready(wbck_alu_ready),
    .wbck_o_wdat(alu_wdat),
    .wbck_o_rdidx(alu_rdidx),
    .csr_ena(alu_csr_ena),
    .csr_wr_en(alu_csr_wr_en),
    .csr_rd_en(alu_csr_rd_en),
    .csr_idx(alu_csr_idx),
    .nonflush_cmt_ena(nonflush_cmt_ena_w),
    .csr_access_ilgl(csr_access_ilgl_w),
    .read_csr_dat(read_csr_dat_w),
    .wbck_csr_dat(alu_wbck_csr_dat),
    .agu_icb_cmd_valid(agu_icb_cmd_valid),
    .agu_icb_cmd_ready(agu_icb_cmd_ready),
    .agu_icb_cmd_addr(agu_icb_cmd_addr),
    .agu_icb_cmd_read(agu_icb_cmd_read),
    .agu_icb_cmd_wdata(agu_icb_cmd_wdata),
    .agu_icb_cmd_wmask(agu_icb_cmd_wmask),
    .agu_icb_cmd_lock(agu_icb_cmd_lock),
    .agu_icb_cmd_excl(agu_icb_cmd_excl),
    .agu_icb_cmd_size(agu_icb_cmd_size),
    .agu_icb_cmd_back2agu(agu_icb_cmd_back2agu),
    .agu_icb_cmd_usign(agu_icb_cmd_usign),
    .agu_icb_cmd_itag(agu_icb_cmd_itag),
    .agu_icb_rsp_valid(agu_icb_rsp_valid),
    .agu_icb_rsp_ready(agu_icb_rsp_ready),
    .agu_icb_rsp_err(agu_icb_rsp_err),
    .agu_icb_rsp_excl_ok(agu_icb_rsp_excl_ok),
    .agu_icb_rsp_rdata(agu_icb_rsp_rdata),
    .nice_req_valid(nice_req_valid),
    .nice_req_ready(nice_req_ready),
    .nice_req_instr(nice_req_inst),
    .nice_req_rs1(nice_req_rs1),
    .nice_req_rs2(nice_req_rs2),
    .nice_rsp_multicyc_valid(nice_rsp_multicyc_valid),
    .nice_rsp_multicyc_ready(nice_rsp_multicyc_ready),
    .nice_longp_wbck_valid(alu_nice_longp_wbck_valid),
    .nice_longp_wbck_ready(alu_nice_longp_wbck_ready),
    .nice_o_itag(alu_nice_o_itag)
  );
  e203_exu_longpwbck longp_u (
    .clk(clk),
    .rst_n(rst_n),
    .lsu_wbck_i_valid(lsu_o_valid),
    .lsu_wbck_i_ready(longp_lsu_ready),
    .lsu_wbck_i_wdat(lsu_o_wbck_wdat),
    .lsu_wbck_i_itag(1'(lsu_o_wbck_itag)),
    .lsu_wbck_i_err(lsu_o_wbck_err),
    .lsu_cmt_i_buserr(lsu_o_cmt_buserr),
    .lsu_cmt_i_badaddr(lsu_o_cmt_badaddr),
    .lsu_cmt_i_ld(lsu_o_cmt_ld),
    .lsu_cmt_i_st(lsu_o_cmt_st),
    .longp_wbck_o_valid(longp_wbck_valid),
    .longp_wbck_o_ready(wbck_longp_ready),
    .longp_wbck_o_wdat(longp_wbck_wdat),
    .longp_wbck_o_flags(longp_wbck_flags),
    .longp_wbck_o_rdidx(longp_wbck_rdidx),
    .longp_wbck_o_rdfpu(longp_wbck_rdfpu),
    .longp_excp_o_valid(longp_excp_valid),
    .longp_excp_o_ready(longp_excp_ready_from_commit),
    .longp_excp_o_insterr(longp_excp_insterr),
    .longp_excp_o_ld(longp_excp_ld),
    .longp_excp_o_st(longp_excp_st),
    .longp_excp_o_buserr(longp_excp_buserr),
    .longp_excp_o_badaddr(longp_excp_badaddr),
    .longp_excp_o_pc(longp_excp_pc),
    .oitf_empty(oitf_is_empty),
    .oitf_ret_ptr(oitf_ret_ptr),
    .oitf_ret_rdidx(oitf_ret_rdidx),
    .oitf_ret_pc(oitf_ret_pc),
    .oitf_ret_rdwen(oitf_ret_rdwen),
    .oitf_ret_rdfpu(oitf_ret_rdfpu),
    .oitf_ret_ena(oitf_ret_ena),
    .nice_longp_wbck_i_valid(1'b0),
    .nice_longp_wbck_i_ready(longp_nice_ready),
    .nice_longp_wbck_i_wdat(0),
    .nice_longp_wbck_i_itag(0),
    .nice_longp_wbck_i_err(1'b0)
  );
  e203_exu_wbck wbck_u (
    .clk(clk),
    .rst_n(rst_n),
    .alu_wbck_i_valid(alu_done_valid),
    .alu_wbck_i_ready(wbck_alu_ready),
    .alu_wbck_i_wdat(alu_wdat),
    .alu_wbck_i_rdidx(alu_rdidx),
    .longp_wbck_i_valid(longp_wbck_valid),
    .longp_wbck_i_ready(wbck_longp_ready),
    .longp_wbck_i_wdat(longp_wbck_wdat),
    .longp_wbck_i_flags(0),
    .longp_wbck_i_rdidx(longp_wbck_rdidx),
    .longp_wbck_i_rdfpu(1'b0),
    .rf_wbck_o_ena(wbck_rf_ena),
    .rf_wbck_o_wdat(wbck_rf_wdat),
    .rf_wbck_o_rdidx(wbck_rf_rdidx)
  );
  e203_exu_commit commit_u (
    .clk(clk),
    .rst_n(rst_n),
    .commit_mret(commit_mret_w),
    .commit_trap(commit_trap_w),
    .core_wfi(core_wfi_w),
    .nonflush_cmt_ena(nonflush_cmt_ena_w),
    .excp_active(excp_active_w),
    .amo_wait(amo_wait_w),
    .wfi_halt_ifu_req(wfi_halt_ifu_req_w),
    .wfi_halt_exu_req(wfi_halt_exu_req_w),
    .wfi_halt_ifu_ack(wfi_halt_ifu_ack),
    .wfi_halt_exu_ack(disp_wfi_halt_exu_ack),
    .dbg_irq_r(dbg_irq_r),
    .lcl_irq_r(lcl_irq_r),
    .ext_irq_r(ext_irq_r),
    .sft_irq_r(sft_irq_r),
    .tmr_irq_r(tmr_irq_r),
    .evt_r(evt_r),
    .status_mie_r(csr_status_mie),
    .mtie_r(csr_mtie),
    .msie_r(csr_msie),
    .meie_r(csr_meie),
    .alu_cmt_i_valid(alu_cmt_valid),
    .alu_cmt_i_ready(commit_alu_ready),
    .alu_cmt_i_pc(alu_cmt_pc),
    .alu_cmt_i_instr(alu_cmt_instr),
    .alu_cmt_i_pc_vld(alu_cmt_pc_vld),
    .alu_cmt_i_imm(alu_cmt_imm),
    .alu_cmt_i_rv32(alu_cmt_rv32),
    .alu_cmt_i_bjp(alu_cmt_bjp),
    .alu_cmt_i_wfi(alu_cmt_wfi),
    .alu_cmt_i_fencei(alu_cmt_fencei),
    .alu_cmt_i_mret(alu_cmt_mret),
    .alu_cmt_i_dret(alu_cmt_dret),
    .alu_cmt_i_ecall(alu_cmt_ecall),
    .alu_cmt_i_ebreak(alu_cmt_ebreak),
    .alu_cmt_i_ifu_misalgn(alu_cmt_ifu_misalgn),
    .alu_cmt_i_ifu_buserr(alu_cmt_ifu_buserr),
    .alu_cmt_i_ifu_ilegl(alu_cmt_ifu_ilegl),
    .alu_cmt_i_bjp_prdt(alu_cmt_bjp_prdt),
    .alu_cmt_i_bjp_rslv(alu_cmt_bjp_rslv),
    .alu_cmt_i_misalgn(alu_cmt_misalgn),
    .alu_cmt_i_ld(alu_cmt_ld),
    .alu_cmt_i_stamo(alu_cmt_stamo),
    .alu_cmt_i_buserr(alu_cmt_buserr),
    .alu_cmt_i_badaddr(alu_cmt_badaddr),
    .cmt_badaddr(cmt_badaddr_w),
    .cmt_badaddr_ena(cmt_badaddr_ena_w),
    .cmt_epc(cmt_epc_w),
    .cmt_epc_ena(cmt_epc_ena_w),
    .cmt_cause(cmt_cause_w),
    .cmt_cause_ena(cmt_cause_ena_w),
    .cmt_instret_ena(cmt_instret_ena_w),
    .cmt_status_ena(cmt_status_ena_w),
    .cmt_dpc(cmt_dpc_w),
    .cmt_dpc_ena(cmt_dpc_ena_w),
    .cmt_dcause(cmt_dcause_w),
    .cmt_dcause_ena(cmt_dcause_ena_w),
    .cmt_mret_ena(cmt_mret_ena_w),
    .csr_epc_r(csr_mepc_val),
    .csr_dpc_r(csr_dpc_val),
    .csr_mtvec_r(csr_mtvec_val),
    .dbg_mode(dbg_mode),
    .dbg_halt_r(dbg_halt_r),
    .dbg_step_r(dbg_step_r),
    .dbg_ebreakm_r(dbg_ebreakm_r),
    .oitf_empty(oitf_is_empty),
    .u_mode(csr_u_mode),
    .s_mode(csr_s_mode),
    .h_mode(csr_h_mode),
    .m_mode(csr_m_mode),
    .longp_excp_i_ready(longp_excp_ready_from_commit),
    .longp_excp_i_valid(longp_excp_valid),
    .longp_excp_i_ld(longp_excp_ld),
    .longp_excp_i_st(longp_excp_st),
    .longp_excp_i_buserr(longp_excp_buserr),
    .longp_excp_i_badaddr(longp_excp_badaddr),
    .longp_excp_i_insterr(longp_excp_insterr),
    .longp_excp_i_pc(longp_excp_pc),
    .flush_pulse(flush_pulse_w),
    .flush_req(flush_req_w),
    .pipe_flush_ack(pipe_flush_ack),
    .pipe_flush_req(pipe_flush_req_w),
    .pipe_flush_add_op1(pipe_flush_add_op1_w),
    .pipe_flush_add_op2(pipe_flush_add_op2_w),
    .pipe_flush_pc(pipe_flush_pc_w)
  );
  e203_exu_csr csr_u (
    .clk(clk),
    .rst_n(rst_n),
    .clk_aon(clk_aon),
    .nonflush_cmt_ena(nonflush_cmt_ena_w),
    .csr_ena(alu_csr_ena),
    .csr_wr_en(alu_csr_wr_en),
    .csr_rd_en(alu_csr_rd_en),
    .csr_idx(alu_csr_idx),
    .csr_access_ilgl(csr_access_ilgl_w),
    .read_csr_dat(csr_rdata),
    .wbck_csr_dat(alu_wbck_csr_dat),
    .nice_xs_off(csr_nice_xs_off),
    .tm_stop(csr_tm_stop),
    .core_cgstop(csr_core_cgstop),
    .tcm_cgstop(csr_tcm_cgstop),
    .itcm_nohold(csr_itcm_nohold),
    .mdv_nob2b(csr_mdv_nob2b),
    .core_mhartid(core_mhartid[0:0]),
    .ext_irq_r(ext_irq_r),
    .sft_irq_r(sft_irq_r),
    .tmr_irq_r(tmr_irq_r),
    .status_mie_r(csr_status_mie),
    .mtie_r(csr_mtie),
    .msie_r(csr_msie),
    .meie_r(csr_meie),
    .wr_dcsr_ena(csr_wr_dcsr_ena),
    .wr_dpc_ena(csr_wr_dpc_ena),
    .wr_dscratch_ena(csr_wr_dscratch_ena),
    .dcsr_r(dcsr_r),
    .dpc_r(dpc_r),
    .dscratch_r(dscratch_r),
    .wr_csr_nxt(csr_wr_csr_nxt),
    .dbg_mode(dbg_mode),
    .dbg_stopcycle(dbg_stopcycle),
    .u_mode(csr_u_mode),
    .s_mode(csr_s_mode),
    .h_mode(csr_h_mode),
    .m_mode(csr_m_mode),
    .cmt_badaddr(cmt_badaddr_w),
    .cmt_badaddr_ena(cmt_badaddr_ena_w),
    .cmt_epc(cmt_epc_w),
    .cmt_epc_ena(cmt_epc_ena_w),
    .cmt_cause(cmt_cause_w),
    .cmt_cause_ena(cmt_cause_ena_w),
    .cmt_status_ena(cmt_status_ena_w),
    .cmt_instret_ena(cmt_instret_ena_w),
    .cmt_mret_ena(cmt_mret_ena_w),
    .csr_epc_r(csr_mepc_val),
    .csr_dpc_r(csr_dpc_val),
    .csr_mtvec_r(csr_mtvec_val)
  );
  e203_exu_regfile rf_u (
    .clk(clk),
    .rst_n(rst_n),
    .test_mode(test_mode),
    .read_src1_idx(dec_rs1_idx),
    .read_src1_dat(rf_rs1_data),
    .read_src2_idx(dec_rs2_idx),
    .read_src2_dat(rf_rs2_data),
    .wbck_dest_wen(wbck_rf_ena),
    .wbck_dest_idx(wbck_rf_rdidx),
    .wbck_dest_dat(wbck_rf_wdat),
    .x1_r(rf_x1_r)
  );
  assign disp_valid_gated = i_valid;
  assign i_ready = disp_rdy;
  assign pipe_flush_req = pipe_flush_req_w;
  assign pipe_flush_add_op1 = pipe_flush_add_op1_w;
  assign pipe_flush_add_op2 = pipe_flush_add_op2_w;
  assign pipe_flush_pc = pipe_flush_pc_w;
  assign lsu_o_ready = longp_lsu_ready;
  assign oitf_empty = oitf_is_empty;
  assign rf2ifu_x1 = rf_x1_r;
  assign rf2ifu_rs1 = rf_rs1_data;
  assign dec2ifu_rden = dec_rd_en & ~disp_oitf_rdfpu_w;
  assign dec2ifu_rs1en = dec_rs1_en & ~disp_oitf_rs1fpu_w;
  assign dec2ifu_rdidx = dec_rd_idx;
  assign dec2ifu_mulhsu = dec_mulhsu;
  assign dec2ifu_div = dec_div;
  assign dec2ifu_rem = dec_rem;
  assign dec2ifu_divu = dec_divu;
  assign dec2ifu_remu = dec_remu;
  assign exu_active = ~oitf_is_empty | i_valid | excp_active_w;
  assign excp_active = excp_active_w;
  assign commit_mret = commit_mret_w;
  assign commit_trap = commit_trap_w;
  assign core_wfi = core_wfi_w;
  assign wfi_halt_ifu_req = wfi_halt_ifu_req_w;
  assign tm_stop = csr_tm_stop;
  assign itcm_nohold = csr_itcm_nohold;
  assign core_cgstop = csr_core_cgstop;
  assign tcm_cgstop = csr_tcm_cgstop;
  assign cmt_dpc = cmt_dpc_w;
  assign cmt_dpc_ena = cmt_dpc_ena_w;
  assign cmt_dcause = cmt_dcause_w;
  assign cmt_dcause_ena = cmt_dcause_ena_w;
  assign wr_dcsr_ena = csr_wr_dcsr_ena;
  assign wr_dpc_ena = csr_wr_dpc_ena;
  assign wr_dscratch_ena = csr_wr_dscratch_ena;
  assign wr_csr_nxt = csr_wr_csr_nxt;
  assign alu_done_valid = alu_wbck_valid;
  assign oitf_empty_w = oitf_is_empty;
  assign pipe_flush_pulse_w = flush_pulse_w;
  assign read_csr_dat_w = csr_rdata;

endmodule

