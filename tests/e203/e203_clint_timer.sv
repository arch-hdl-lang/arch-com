// E203 HBirdv2 CLINT Timer (12th E203 benchmark)
// 64-bit mtime counter with mtimecmp comparison for timer interrupt.
// APB-like register interface: read/write mtime and mtimecmp as two
// 32-bit halves (low at offset 0, high at offset 4).
// Exercises: {a,b} concat for 64-bit assembly, elsif chains, seq blocks.
// domain SysDomain
//   freq_mhz: 100

module ClintTimer #(
  parameter int XLEN = 32
) (
  input logic clk,
  input logic rst,
  input logic [4-1:0] reg_addr,
  input logic [32-1:0] reg_wdata,
  input logic reg_wen,
  output logic [32-1:0] reg_rdata,
  output logic tmr_irq
);

  // Register read/write interface
  // 0=mtime_lo, 4=mtime_hi, 8=mtimecmp_lo, C=mtimecmp_hi
  // Timer interrupt output
  // 64-bit mtime counter (split into two 32-bit regs)
  logic [32-1:0] mtime_lo_r = 0;
  logic [32-1:0] mtime_hi_r = 0;
  // 64-bit mtimecmp register
  logic [32-1:0] mtimecmp_lo_r = 'hFFFFFFFF;
  logic [32-1:0] mtimecmp_hi_r = 'hFFFFFFFF;
  // Assemble 64-bit values using concat
  logic [64-1:0] mtime_full;
  assign mtime_full = {mtime_hi_r, mtime_lo_r};
  logic [64-1:0] mtimecmp_full;
  assign mtimecmp_full = {mtimecmp_hi_r, mtimecmp_lo_r};
  // Increment: mtime + 1 with carry
  logic [64-1:0] mtime_inc;
  assign mtime_inc = 64'((mtime_full + 1));
  logic [32-1:0] next_lo;
  assign next_lo = mtime_inc[31:0];
  logic [32-1:0] next_hi;
  assign next_hi = mtime_inc[63:32];
  // Timer interrupt: mtime >= mtimecmp (unsigned 64-bit compare)
  logic irq_pending;
  assign irq_pending = ((mtime_full >= mtimecmp_full)) ? (1'b1) : (1'b0);
  // Counter update and register writes
  always_ff @(posedge clk) begin
    if (rst) begin
      mtime_hi_r <= 0;
      mtime_lo_r <= 0;
      mtimecmp_hi_r <= 'hFFFFFFFF;
      mtimecmp_lo_r <= 'hFFFFFFFF;
    end else begin
      mtime_lo_r <= next_lo;
      mtime_hi_r <= next_hi;
      if (reg_wen) begin
        if ((reg_addr == 'h0)) begin
          mtime_lo_r <= reg_wdata;
        end else if ((reg_addr == 'h4)) begin
          mtime_hi_r <= reg_wdata;
        end else if ((reg_addr == 'h8)) begin
          mtimecmp_lo_r <= reg_wdata;
        end else if ((reg_addr == 'hC)) begin
          mtimecmp_hi_r <= reg_wdata;
        end
      end
    end
  end
  // Default: increment mtime every cycle
  // Register writes override counter
  // Register read mux
  always_comb begin
    if ((reg_addr == 'h0)) begin
      reg_rdata = mtime_lo_r;
    end else if ((reg_addr == 'h4)) begin
      reg_rdata = mtime_hi_r;
    end else if ((reg_addr == 'h8)) begin
      reg_rdata = mtimecmp_lo_r;
    end else if ((reg_addr == 'hC)) begin
      reg_rdata = mtimecmp_hi_r;
    end else begin
      reg_rdata = 0;
    end
    tmr_irq = irq_pending;
  end

endmodule

