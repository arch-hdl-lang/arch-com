// E203 HBirdv2 Load-Store Unit Controller (11th E203 benchmark)
// Handles load/store memory requests with byte/halfword/word access.
// Load path: reads memory, aligns and sign-extends based on funct3.
// Store path: generates byte-enable mask and aligned write data.
// Exercises: {a,b} concat, {N{expr}} repeat, elsif, let chains.
//
// funct3 encoding (RV32I):
//   000 = LB/SB  (byte)
//   001 = LH/SH  (halfword)
//   010 = LW/SW  (word)
//   100 = LBU    (byte unsigned)
//   101 = LHU    (halfword unsigned)
// domain SysDomain
//   freq_mhz: 100

module LsuCtrl #(
  parameter int XLEN = 32
) (
  input logic [32-1:0] addr,
  input logic [32-1:0] wdata,
  input logic [3-1:0] funct3,
  input logic is_load,
  input logic is_store,
  output logic [32-1:0] mem_addr,
  output logic [32-1:0] mem_wdata,
  output logic [4-1:0] mem_wstrb,
  output logic mem_wen,
  input logic [32-1:0] mem_rdata,
  output logic [32-1:0] load_result
);

  // Request from EXU
  // byte address
  // store data (unaligned)
  // access type
  // Memory interface (aligned, word-addressed)
  // byte enables
  // Load result (from memory read data)
  // aligned word from memory
  // ── Address alignment ──────────────────────────────────────
  logic [2-1:0] byte_off;
  assign byte_off = addr[1:0];
  logic [32-1:0] word_addr;
  assign word_addr = {addr[31:2], {2{1'b0}}};
  // ── funct3 decode ──────────────────────────────────────────
  logic is_byte;
  assign is_byte = (funct3[1:0] == 0);
  logic is_half;
  assign is_half = (funct3[1:0] == 1);
  logic is_word;
  assign is_word = (funct3[1:0] == 2);
  logic is_unsigned;
  assign is_unsigned = (funct3[2:2] != 0);
  // ── Store: byte-enable and data alignment ──────────────────
  logic [4-1:0] wstrb_byte;
  assign wstrb_byte = ((byte_off == 0)) ? ('h1) : (((byte_off == 1)) ? ('h2) : (((byte_off == 2)) ? ('h4) : ('h8)));
  logic [4-1:0] wstrb_half;
  assign wstrb_half = ((byte_off[1:1] == 0)) ? ('h3) : ('hC);
  logic [4-1:0] wstrb_word;
  assign wstrb_word = 'hF;
  logic [4-1:0] store_strb;
  assign store_strb = (is_byte) ? (wstrb_byte) : ((is_half) ? (wstrb_half) : (wstrb_word));
  // Shift store data to correct byte lane
  logic [8-1:0] wdata_byte;
  assign wdata_byte = wdata[7:0];
  logic [16-1:0] wdata_half;
  assign wdata_half = wdata[15:0];
  logic [32-1:0] store_data;
  assign store_data = (is_byte) ? (32'((32'($unsigned(wdata_byte)) << 32'((32'($unsigned(byte_off)) << 3))))) : ((is_half) ? (32'((32'($unsigned(wdata_half)) << 32'((32'($unsigned(byte_off[1:1])) << 4))))) : (wdata));
  // ── Load: extract and sign-extend ──────────────────────────
  // Select the right byte from the word
  logic [32-1:0] rdata_shifted;
  assign rdata_shifted = 32'((mem_rdata >> 32'((32'($unsigned(byte_off)) << 3))));
  logic [8-1:0] load_byte;
  assign load_byte = rdata_shifted[7:0];
  logic [16-1:0] load_half;
  assign load_half = rdata_shifted[15:0];
  // Sign extension using {N{sign_bit}}
  logic byte_sign;
  assign byte_sign = (load_byte[7:7] != 0);
  logic half_sign;
  assign half_sign = (load_half[15:15] != 0);
  logic [32-1:0] lb_result;
  assign lb_result = {{24{byte_sign}}, load_byte};
  logic [32-1:0] lbu_result;
  assign lbu_result = {{24{1'b0}}, load_byte};
  logic [32-1:0] lh_result;
  assign lh_result = {{16{half_sign}}, load_half};
  logic [32-1:0] lhu_result;
  assign lhu_result = {{16{1'b0}}, load_half};
  logic [32-1:0] load_val;
  assign load_val = (is_byte) ? ((is_unsigned) ? (lbu_result) : (lb_result)) : ((is_half) ? ((is_unsigned) ? (lhu_result) : (lh_result)) : (mem_rdata));
  // ── Output drives ──────────────────────────────────────────
  assign mem_addr = word_addr;
  assign mem_wdata = store_data;
  assign mem_wstrb = (is_store) ? (store_strb) : (0);
  assign mem_wen = is_store;
  assign load_result = load_val;

endmodule

