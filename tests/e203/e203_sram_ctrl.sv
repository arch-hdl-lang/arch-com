// E203 SRAM Controller
// Provides a simple ICB-slave interface to a synchronous SRAM.
// Single-port RAM with read and write sharing the same port.
// domain SysDomain
//   freq_mhz: 100

module SramBank #(
  parameter int DEPTH = 4096,
  parameter int DATA_WIDTH = 32
) (
  input logic clk,
  input logic rw_en,
  input logic rw_wen,
  input logic [12-1:0] rw_addr,
  input logic [DATA_WIDTH-1:0] rw_wdata,
  output logic [DATA_WIDTH-1:0] rw_rdata
);

  logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];
  logic [DATA_WIDTH-1:0] rw_rdata_r;
  
  always_ff @(posedge clk) begin
    if (rw_en) begin
      if (rw_wen)
        mem[rw_addr] <= rw_wdata;
      else
        rw_rdata_r <= mem[rw_addr];
    end
  end
  assign rw_rdata = rw_rdata_r;

endmodule

module SramCtrl #(
  parameter int DEPTH = 4096
) (
  input logic clk,
  input logic rst_n,
  input logic icb_cmd_valid,
  output logic icb_cmd_ready,
  input logic [32-1:0] icb_cmd_addr,
  input logic [32-1:0] icb_cmd_wdata,
  input logic [4-1:0] icb_cmd_wmask,
  input logic icb_cmd_read,
  output logic icb_rsp_valid,
  input logic icb_rsp_ready,
  output logic [32-1:0] icb_rsp_rdata,
  output logic icb_rsp_err
);

  // ICB slave interface
  // Word address from byte address
  logic [12-1:0] word_addr;
  assign word_addr = icb_cmd_addr[13:2];
  // SRAM instance
  logic [32-1:0] sram_rdata;
  SramBank mem (
    .clk(clk),
    .rw_en(icb_cmd_valid),
    .rw_wen((~icb_cmd_read)),
    .rw_addr(word_addr),
    .rw_wdata(icb_cmd_wdata),
    .rw_rdata(sram_rdata)
  );
  // Response valid: 1-cycle latency
  logic rsp_valid_r = 1'b0;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      rsp_valid_r <= 1'b0;
    end else begin
      rsp_valid_r <= (icb_cmd_valid & icb_cmd_ready);
    end
  end
  assign icb_cmd_ready = 1'b1;
  assign icb_rsp_valid = rsp_valid_r;
  assign icb_rsp_rdata = sram_rdata;
  assign icb_rsp_err = 1'b0;

endmodule

