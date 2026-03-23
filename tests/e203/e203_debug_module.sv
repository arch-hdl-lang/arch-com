// E203 Debug Module (simplified)
// JTAG-like debug interface for halt/resume/register access.
// Implements a minimal set of Debug Module registers per RISC-V Debug Spec 0.13:
//   dmcontrol(0x10): haltreq, resumereq, dmactive
//   dmstatus(0x11): halted, running, allhalted
//   data0(0x04): abstract data register
//   command(0x17): access register command
// Interface: APB slave for debug transport module (DTM).
// domain SysDomain
//   freq_mhz: 100

module DebugModule (
  input logic clk,
  input logic rst_n,
  input logic psel,
  input logic penable,
  input logic [32-1:0] paddr,
  input logic [32-1:0] pwdata,
  input logic pwrite,
  output logic [32-1:0] prdata,
  output logic pready,
  input logic hart_halted,
  input logic hart_running,
  output logic halt_req,
  output logic resume_req,
  output logic [16-1:0] dbg_reg_addr,
  output logic [32-1:0] dbg_reg_wdata,
  output logic dbg_reg_wen,
  input logic [32-1:0] dbg_reg_rdata
);

  // APB slave (from Debug Transport Module)
  // Core interface
  // core reports halted
  // core reports running
  // request core to halt
  // request core to resume
  // register access address
  // register write data
  // register write enable
  // register read data
  // ── DM registers ───────────────────────────────────────────────
  logic dmactive_r = 1'b0;
  logic haltreq_r = 1'b0;
  logic resumereq_r = 1'b0;
  logic [32-1:0] data0_r = 0;
  logic cmd_valid_r = 1'b0;
  logic [16-1:0] cmd_reg_r = 0;
  logic cmd_write_r = 1'b0;
  logic [8-1:0] reg_off;
  assign reg_off = paddr[7:0];
  logic apb_wr;
  assign apb_wr = ((psel & penable) & pwrite);
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      cmd_reg_r <= 0;
      cmd_valid_r <= 1'b0;
      cmd_write_r <= 1'b0;
      data0_r <= 0;
      dmactive_r <= 1'b0;
      haltreq_r <= 1'b0;
      resumereq_r <= 1'b0;
    end else begin
      if (resumereq_r) begin
        resumereq_r <= 1'b0;
      end
      if (cmd_valid_r) begin
        cmd_valid_r <= 1'b0;
      end
      if (apb_wr) begin
        if ((reg_off == 'h10)) begin
          dmactive_r <= (pwdata[0:0] != 0);
          haltreq_r <= (pwdata[31:31] != 0);
          resumereq_r <= (pwdata[30:30] != 0);
        end else if ((reg_off == 'h4)) begin
          data0_r <= pwdata;
        end else if ((reg_off == 'h17)) begin
          cmd_valid_r <= 1'b1;
          cmd_reg_r <= pwdata[15:0];
          cmd_write_r <= (pwdata[16:16] != 0);
        end
      end
      if ((cmd_valid_r & (~cmd_write_r))) begin
        data0_r <= dbg_reg_rdata;
      end
    end
  end
  // Auto-clear resume request after 1 cycle
  // Auto-clear command after 1 cycle
  // dmcontrol (0x10)
  // data0 (0x04)
  // command (0x17)
  // Capture register read result into data0
  always_comb begin
    halt_req = (haltreq_r & dmactive_r);
    resume_req = (resumereq_r & dmactive_r);
    dbg_reg_addr = cmd_reg_r;
    dbg_reg_wdata = data0_r;
    dbg_reg_wen = (cmd_valid_r & cmd_write_r);
    if ((reg_off == 'h10)) begin
      prdata = {haltreq_r, resumereq_r, {29{1'b0}}, dmactive_r};
    end else if ((reg_off == 'h11)) begin
      prdata = {{22{1'b0}}, hart_halted, hart_halted, {4{1'b0}}, hart_running, hart_running, {2{1'b0}}};
    end else if ((reg_off == 'h4)) begin
      prdata = data0_r;
    end else if ((reg_off == 'h12)) begin
      prdata = {cmd_valid_r, {31{1'b0}}};
    end else begin
      prdata = 0;
    end
    pready = 1'b1;
  end

endmodule

// APB read
// dmcontrol
// dmstatus: bit 9=allhalted, bit 8=anyhalted, bit 3=allrunning, bit 2=anyrunning
// abstractcs: busy bit (bit 12)
