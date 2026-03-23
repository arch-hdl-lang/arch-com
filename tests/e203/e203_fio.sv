// E203 Fast I/O Port
// Provides a low-latency ICB-to-memory-mapped I/O interface.
// Single-cycle read/write to a small register file (16 registers).
// Used for performance-critical peripherals (e.g., GPIO fast set/clear).
// domain SysDomain
//   freq_mhz: 100

module Fio #(
  parameter int NUM_REGS = 16
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
  output logic icb_rsp_err,
  output logic [32-1:0] fio_out_0,
  output logic [32-1:0] fio_out_1,
  output logic [32-1:0] fio_out_2,
  output logic [32-1:0] fio_out_3,
  input logic [32-1:0] fio_in_0,
  input logic [32-1:0] fio_in_1
);

  // ICB slave interface
  // Fast I/O output pins (directly mapped from regs)
  // Fast I/O input pins (directly readable)
  // Register file (word-addressed, addr[5:2] selects register)
  logic [4-1:0] reg_idx;
  assign reg_idx = icb_cmd_addr[5:2];
  // Output registers
  logic [32-1:0] fio_r0 = 0;
  logic [32-1:0] fio_r1 = 0;
  logic [32-1:0] fio_r2 = 0;
  logic [32-1:0] fio_r3 = 0;
  // Response pipeline (1-cycle)
  logic rsp_valid_r = 1'b0;
  logic [32-1:0] rsp_rdata_r = 0;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      fio_r0 <= 0;
      fio_r1 <= 0;
      fio_r2 <= 0;
      fio_r3 <= 0;
      rsp_rdata_r <= 0;
      rsp_valid_r <= 1'b0;
    end else begin
      rsp_valid_r <= icb_cmd_valid;
      if ((icb_cmd_valid & (~icb_cmd_read))) begin
        if ((reg_idx == 0)) begin
          fio_r0 <= icb_cmd_wdata;
        end else if ((reg_idx == 1)) begin
          fio_r1 <= icb_cmd_wdata;
        end else if ((reg_idx == 2)) begin
          fio_r2 <= icb_cmd_wdata;
        end else if ((reg_idx == 3)) begin
          fio_r3 <= icb_cmd_wdata;
        end
      end
      if ((icb_cmd_valid & icb_cmd_read)) begin
        if ((reg_idx == 0)) begin
          rsp_rdata_r <= fio_r0;
        end else if ((reg_idx == 1)) begin
          rsp_rdata_r <= fio_r1;
        end else if ((reg_idx == 2)) begin
          rsp_rdata_r <= fio_r2;
        end else if ((reg_idx == 3)) begin
          rsp_rdata_r <= fio_r3;
        end else if ((reg_idx == 8)) begin
          rsp_rdata_r <= fio_in_0;
        end else if ((reg_idx == 9)) begin
          rsp_rdata_r <= fio_in_1;
        end else begin
          rsp_rdata_r <= 0;
        end
      end
    end
  end
  // Write
  // Latch read data
  assign icb_cmd_ready = 1'b1;
  assign icb_rsp_valid = rsp_valid_r;
  assign icb_rsp_rdata = rsp_rdata_r;
  assign icb_rsp_err = 1'b0;
  assign fio_out_0 = fio_r0;
  assign fio_out_1 = fio_r1;
  assign fio_out_2 = fio_r2;
  assign fio_out_3 = fio_r3;

endmodule

