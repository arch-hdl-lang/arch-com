module microcode_sequencer (
  input logic clk,
  input logic c_n_in,
  input logic c_inc_in,
  input logic r_en,
  input logic cc,
  input logic ien,
  input logic [4-1:0] d_in,
  input logic [5-1:0] instr_in,
  input logic oen,
  output logic [4-1:0] d_out,
  output logic c_n_out,
  output logic c_inc_out,
  output logic full,
  output logic empty
);

  // ---- Instruction Decoder (combinational) ----
  logic dec_cen;
  logic dec_rst;
  logic dec_oen;
  logic dec_inc;
  logic dec_rsel;
  logic dec_rce;
  logic dec_pc_mux_sel;
  logic [2-1:0] dec_a_mux_sel;
  logic [2-1:0] dec_b_mux_sel;
  logic dec_push;
  logic dec_pop;
  logic dec_src_sel;
  logic dec_stack_we;
  logic dec_stack_re;
  logic dec_out_ce;
  always_comb begin
    dec_cen = 1'b0;
    dec_rst = 1'b0;
    dec_oen = 1'b0;
    dec_inc = 1'b0;
    dec_rsel = 1'b0;
    dec_rce = 1'b0;
    dec_pc_mux_sel = 1'b0;
    dec_a_mux_sel = 0;
    dec_b_mux_sel = 0;
    dec_push = 1'b0;
    dec_pop = 1'b0;
    dec_src_sel = 1'b0;
    dec_stack_we = 1'b0;
    dec_stack_re = 1'b0;
    dec_out_ce = 1'b0;
    if (~ien & ~cc) begin
      if (instr_in == 0) begin
        dec_rst = 1'b1;
        dec_oen = 1'b1;
        dec_inc = 1'b1;
        dec_a_mux_sel = 2;
        dec_b_mux_sel = 2;
        dec_out_ce = 1'b1;
      end else if (instr_in == 1) begin
        dec_oen = 1'b1;
        dec_inc = 1'b1;
        dec_a_mux_sel = 2;
        dec_b_mux_sel = 0;
        dec_out_ce = 1'b1;
      end else if (instr_in == 2) begin
        dec_oen = 1'b1;
        dec_inc = 1'b1;
        dec_a_mux_sel = 1;
        dec_b_mux_sel = 2;
        dec_out_ce = 1'b1;
      end else if (instr_in == 3) begin
        dec_oen = 1'b1;
        dec_inc = 1'b1;
        dec_a_mux_sel = 0;
        dec_b_mux_sel = 2;
        dec_out_ce = 1'b1;
      end else if (instr_in == 4) begin
        dec_cen = 1'b1;
        dec_oen = 1'b1;
        dec_inc = 1'b1;
        dec_a_mux_sel = 0;
        dec_b_mux_sel = 3;
        dec_out_ce = 1'b1;
      end else if (instr_in == 11) begin
        dec_oen = 1'b1;
        dec_inc = 1'b1;
        dec_push = 1'b1;
        dec_src_sel = 1'b0;
        dec_stack_we = 1'b1;
        dec_a_mux_sel = 2;
        dec_b_mux_sel = 0;
        dec_out_ce = 1'b1;
      end else if (instr_in == 14) begin
        dec_oen = 1'b1;
        dec_pop = 1'b1;
        dec_stack_re = 1'b1;
        dec_pc_mux_sel = 1'b1;
        dec_a_mux_sel = 2;
        dec_b_mux_sel = 1;
        dec_out_ce = 1'b1;
      end
    end
  end
  // PRST: reset stack, output 0, load PC
  // Fetch PC: a=0, b=pc
  // Fetch R: a=reg, b=0
  // Fetch D: a=d_in, b=0
  // Fetch R+D: a=d_in, b=reg, carry enabled
  // Push PC: output PC via adder, push to stack
  // Pop PC
  // ---- Stack Pointer (5-bit register) ----
  logic [5-1:0] sp = 0;
  logic sp_full;
  assign sp_full = sp == 16;
  logic sp_empty;
  assign sp_empty = sp == 0;
  assign full = sp_full;
  assign empty = sp_empty;
  always_ff @(posedge clk) begin
    if (dec_rst) begin
      sp <= 0;
    end else if (dec_push & ~sp_full) begin
      sp <= 5'(sp + 1);
    end else if (dec_pop & ~sp_empty) begin
      sp <= 5'(sp - 1);
    end
  end
  // ---- Stack RAM (16 x 4-bit) ----
  logic [4-1:0] stack_mem [0:16-1];
  logic [4-1:0] stack_data_rd = 0;
  logic [4-1:0] pc_out_w;
  logic [4-1:0] stack_write_data;
  always_comb begin
    if (dec_src_sel) begin
      stack_write_data = d_in;
    end else begin
      stack_write_data = pc_out_w;
    end
  end
  always_ff @(posedge clk) begin
    if (dec_stack_we) begin
      stack_mem[sp[3:0]] <= stack_write_data;
    end
    if (dec_stack_re) begin
      stack_data_rd <= stack_mem[sp[3:0]];
    end
  end
  // ---- aux_reg_mux and aux_reg ----
  logic aux_reg_mux_sel_v;
  assign aux_reg_mux_sel_v = dec_rsel & ~r_en;
  logic [4-1:0] fa_sum;
  logic [4-1:0] aux_reg_mux_out;
  logic [4-1:0] aux_reg_r = 0;
  always_comb begin
    if (aux_reg_mux_sel_v) begin
      aux_reg_mux_out = fa_sum;
    end else begin
      aux_reg_mux_out = d_in;
    end
  end
  logic aux_reg_en_v;
  assign aux_reg_en_v = dec_rce | ~r_en;
  always_ff @(posedge clk) begin
    if (aux_reg_en_v) begin
      aux_reg_r <= aux_reg_mux_out;
    end
  end
  // ---- a_mux ----
  logic [4-1:0] a_mux_out;
  always_comb begin
    if (dec_a_mux_sel == 0) begin
      a_mux_out = d_in;
    end else if (dec_a_mux_sel == 1) begin
      a_mux_out = aux_reg_r;
    end else begin
      a_mux_out = 0;
    end
  end
  // ---- b_mux ----
  logic [4-1:0] b_mux_out;
  always_comb begin
    if (dec_b_mux_sel == 0) begin
      b_mux_out = pc_out_w;
    end else if (dec_b_mux_sel == 1) begin
      b_mux_out = stack_data_rd;
    end else if (dec_b_mux_sel == 2) begin
      b_mux_out = 0;
    end else begin
      b_mux_out = aux_reg_r;
    end
  end
  // ---- Full Adder (4-bit with carry) ----
  logic fa_cin;
  always_comb begin
    if (dec_cen) begin
      fa_cin = c_n_in;
    end else begin
      fa_cin = 1'b0;
    end
  end
  // Extend to 5 bits via concat, then add
  logic [5-1:0] a_ext;
  assign a_ext = {1'b0, a_mux_out};
  logic [5-1:0] b_ext;
  assign b_ext = {1'b0, b_mux_out};
  logic [5-1:0] cin_ext;
  assign cin_ext = {1'b0, 1'b0, 1'b0, 1'b0, fa_cin};
  logic [6-1:0] fa_ab;
  assign fa_ab = a_ext + b_ext;
  logic [7-1:0] fa_full_v;
  assign fa_full_v = fa_ab + 6'($unsigned(cin_ext));
  assign fa_sum = fa_full_v[3:0];
  logic fa_cout;
  assign fa_cout = fa_full_v[4:4] == 1;
  // ---- output enable ----
  logic [4-1:0] arith_d_out;
  always_comb begin
    if (dec_oen & ~oen) begin
      arith_d_out = fa_sum;
    end else begin
      arith_d_out = 0;
    end
  end
  // ---- PC mux, incrementer, reg ----
  logic [4-1:0] pc_mux_out;
  always_comb begin
    if (dec_pc_mux_sel) begin
      pc_mux_out = fa_sum;
    end else begin
      pc_mux_out = pc_out_w;
    end
  end
  logic pc_inc_cin;
  always_comb begin
    if (dec_inc) begin
      pc_inc_cin = c_inc_in;
    end else begin
      pc_inc_cin = 1'b0;
    end
  end
  logic [5-1:0] pc_ext;
  assign pc_ext = {1'b0, pc_mux_out};
  logic [5-1:0] pci_ext;
  assign pci_ext = {1'b0, 1'b0, 1'b0, 1'b0, pc_inc_cin};
  logic [6-1:0] pc_inc_result;
  assign pc_inc_result = pc_ext + pci_ext;
  logic [4-1:0] pc_inc_out;
  assign pc_inc_out = pc_inc_result[3:0];
  logic pc_inc_cout;
  assign pc_inc_cout = pc_inc_result[4:4] == 1;
  logic [4-1:0] pc_reg_r = 0;
  always_ff @(posedge clk) begin
    pc_reg_r <= pc_inc_out;
  end
  assign pc_out_w = pc_reg_r;
  // ---- Result Register ----
  logic [4-1:0] result_reg = 0;
  always_ff @(posedge clk) begin
    if (dec_out_ce) begin
      result_reg <= arith_d_out;
    end
  end
  // ---- Output assignments ----
  // d_out: combinational bypass when dec_out_ce active, else hold from register
  always_comb begin
    if (dec_out_ce) begin
      d_out = arith_d_out;
    end else begin
      d_out = result_reg;
    end
    c_n_out = fa_cout;
    c_inc_out = pc_inc_cout;
  end

endmodule

