module apb_dsp_op #(
  parameter int ADDR_WIDTH = 8,
  parameter int DATA_WIDTH = 32
) (
  input logic clk_dsp,
  input logic en_clk_dsp,
  input logic PCLK,
  input logic PRESETn,
  input logic [ADDR_WIDTH-1:0] PADDR,
  input logic PWRITE,
  input logic [DATA_WIDTH-1:0] PWDATA,
  input logic PSEL,
  input logic PENABLE,
  output logic [DATA_WIDTH-1:0] PRDATA,
  output logic PREADY,
  output logic PSLVERR
);

  // APB register bank
  logic [DATA_WIDTH-1:0] reg_operand_a;
  logic [DATA_WIDTH-1:0] reg_operand_b;
  logic [DATA_WIDTH-1:0] reg_operand_c;
  logic [DATA_WIDTH-1:0] reg_operand_o;
  logic [DATA_WIDTH-1:0] reg_ctrl;
  logic [DATA_WIDTH-1:0] reg_wdata_sram;
  logic [DATA_WIDTH-1:0] reg_addr_sram;
  // SRAM: 64 entries of DATA_WIDTH bits
  logic [63:0] [DATA_WIDTH-1:0] sram_mem;
  // SRAM read data register
  logic [DATA_WIDTH-1:0] sram_rdata;
  // DSP operand value registers
  logic [DATA_WIDTH-1:0] dsp_a;
  logic [DATA_WIDTH-1:0] dsp_b;
  logic [DATA_WIDTH-1:0] dsp_c;
  // SRAM address validity (< 64)
  logic sram_addr_valid;
  assign sram_addr_valid = reg_addr_sram[DATA_WIDTH - 1:6] == 0;
  // APB access phase
  logic apb_access;
  assign apb_access = PSEL & PENABLE;
  // Address validity check
  logic addr_valid;
  logic addr_error;
  logic sram_op_active;
  always_comb begin
    addr_valid = (PADDR == 0) | (PADDR == 4) | (PADDR == 8) | (PADDR == 12) | (PADDR == 16) | (PADDR == 20) | (PADDR == 24);
    sram_op_active = (reg_ctrl == 1) | (reg_ctrl == 2) | (reg_ctrl == 3) | (reg_ctrl == 4) | (reg_ctrl == 5) | (reg_ctrl == 6);
    if (apb_access & ~addr_valid) begin
      addr_error = 1'b1;
    end else if (apb_access & sram_op_active & ~sram_addr_valid) begin
      addr_error = 1'b1;
    end else begin
      addr_error = 1'b0;
    end
  end
  // PREADY/PSLVERR hold registers
  logic pready_hold;
  logic pslverr_hold;
  // Always write (unconditional) to avoid Icarus edge cases
  logic next_pready;
  logic next_pslverr;
  always_comb begin
    if (apb_access) begin
      next_pready = 1'b1;
      next_pslverr = addr_error;
    end else begin
      next_pready = pready_hold;
      next_pslverr = pslverr_hold;
    end
  end
  always_ff @(posedge PCLK or negedge PRESETn) begin
    if ((!PRESETn)) begin
      pready_hold <= 1'b0;
      pslverr_hold <= 1'b0;
    end else begin
      pready_hold <= next_pready;
      pslverr_hold <= next_pslverr;
    end
  end
  assign PREADY = pready_hold;
  assign PSLVERR = pslverr_hold;
  // APB write logic
  logic apb_write_en;
  assign apb_write_en = PSEL & PENABLE & PWRITE;
  always_ff @(posedge PCLK or negedge PRESETn) begin
    if ((!PRESETn)) begin
      dsp_a <= 0;
      dsp_b <= 0;
      dsp_c <= 0;
      reg_addr_sram <= 0;
      reg_ctrl <= 0;
      reg_operand_a <= 0;
      reg_operand_b <= 0;
      reg_operand_c <= 0;
      reg_operand_o <= 0;
      reg_wdata_sram <= 0;
      for (int __ri0 = 0; __ri0 < 64; __ri0++) begin
        sram_mem[__ri0] <= 0;
      end
      sram_rdata <= 0;
    end else begin
      if (apb_write_en) begin
        if (PADDR == 0) begin
          reg_operand_a <= PWDATA;
        end else if (PADDR == 4) begin
          reg_operand_b <= PWDATA;
        end else if (PADDR == 8) begin
          reg_operand_c <= PWDATA;
        end else if (PADDR == 12) begin
          reg_operand_o <= PWDATA;
        end else if (PADDR == 16) begin
          reg_ctrl <= PWDATA;
        end else if (PADDR == 20) begin
          reg_wdata_sram <= PWDATA;
        end else if (PADDR == 24) begin
          reg_addr_sram <= PWDATA;
        end
      end
      // SRAM write: when control = 1
      if (reg_ctrl == 1) begin
        if (sram_addr_valid) begin
          sram_mem[reg_addr_sram[5:0]] <= reg_wdata_sram;
        end
      end
      // SRAM read: when control = 2
      if (reg_ctrl == 2) begin
        if (sram_addr_valid) begin
          sram_rdata <= sram_mem[reg_addr_sram[5:0]];
        end
      end
      // DSP read operand A: when control = 3
      if (reg_ctrl == 3) begin
        if (reg_operand_a[DATA_WIDTH - 1:6] == 0) begin
          dsp_a <= sram_mem[reg_operand_a[5:0]];
        end
      end
      // DSP read operand B: when control = 4
      if (reg_ctrl == 4) begin
        if (reg_operand_b[DATA_WIDTH - 1:6] == 0) begin
          dsp_b <= sram_mem[reg_operand_b[5:0]];
        end
      end
      // DSP read operand C: when control = 5
      if (reg_ctrl == 5) begin
        if (reg_operand_c[DATA_WIDTH - 1:6] == 0) begin
          dsp_c <= sram_mem[reg_operand_c[5:0]];
        end
      end
      // DSP write operand O: when control = 6
      if (reg_ctrl == 6) begin
        if (reg_operand_o[DATA_WIDTH - 1:6] == 0) begin
          sram_mem[reg_operand_o[5:0]] <= DATA_WIDTH'(DATA_WIDTH'(dsp_a * dsp_b) + dsp_c);
        end
      end
    end
  end
  // APB read logic (combinational)
  always_comb begin
    if (PADDR == 0) begin
      PRDATA = reg_operand_a;
    end else if (PADDR == 4) begin
      PRDATA = reg_operand_b;
    end else if (PADDR == 8) begin
      PRDATA = reg_operand_c;
    end else if (PADDR == 12) begin
      PRDATA = reg_operand_o;
    end else if (PADDR == 16) begin
      PRDATA = reg_ctrl;
    end else if (PADDR == 20) begin
      PRDATA = reg_wdata_sram;
    end else if (PADDR == 24) begin
      PRDATA = sram_rdata;
    end else begin
      PRDATA = 0;
    end
  end

endmodule

