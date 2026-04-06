// E203 NICE Co-processor Interface
// Routes custom instructions to external NICE accelerator.
// Includes itag FIFO for long-pipe tracking.
module e203_exu_nice (
  input logic clk,
  input logic rst_n,
  input logic nice_i_xs_off,
  input logic nice_i_valid,
  output logic nice_i_ready,
  input logic [32-1:0] nice_i_instr,
  input logic [32-1:0] nice_i_rs1,
  input logic [32-1:0] nice_i_rs2,
  input logic [1-1:0] nice_i_itag,
  output logic nice_o_longpipe,
  output logic nice_o_valid,
  input logic nice_o_ready,
  output logic nice_o_itag_valid,
  input logic nice_o_itag_ready,
  output logic [1-1:0] nice_o_itag,
  input logic nice_rsp_multicyc_valid,
  output logic nice_rsp_multicyc_ready,
  output logic nice_req_valid,
  input logic nice_req_ready,
  output logic [32-1:0] nice_req_instr,
  output logic [32-1:0] nice_req_rs1,
  output logic [32-1:0] nice_req_rs2
);

  // NICE extension disabled
  // Dispatch handshake
  // Output interface
  // Itag writeback (for long-pipe tracking)
  // Multi-cycle response
  // Request to coprocessor
  // Itag FIFO (4-deep, 1-bit wide)
  logic [1-1:0] fifo_mem [4-1:0];
  logic [3-1:0] fifo_wptr;
  logic [3-1:0] fifo_rptr;
  logic fifo_empty;
  assign fifo_empty = fifo_wptr == fifo_rptr;
  logic fifo_full;
  assign fifo_full = fifo_wptr[1:0] == fifo_rptr[1:0] & fifo_wptr[2:2] != fifo_rptr[2:2];
  logic fifo_o_vld;
  assign fifo_o_vld = ~fifo_empty;
  logic [1-1:0] fifo_o_dat;
  assign fifo_o_dat = fifo_mem[fifo_rptr[1:0]];
  logic nice_req_ready_pos;
  assign nice_req_ready_pos = nice_i_xs_off ? 1'b1 : nice_req_ready;
  // FIFO write: when long-pipe request fires
  logic fifo_wen;
  assign fifo_wen = nice_o_longpipe & nice_req_valid & nice_req_ready;
  // FIFO read: when multi-cycle response acknowledged
  logic fifo_ren;
  assign fifo_ren = nice_rsp_multicyc_valid & nice_rsp_multicyc_ready;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      for (int __ri0 = 0; __ri0 < 4; __ri0++) begin
        fifo_mem[__ri0] <= 0;
      end
      fifo_rptr <= 0;
      fifo_wptr <= 0;
    end else begin
      if (fifo_wen) begin
        fifo_mem[fifo_wptr[1:0]] <= nice_i_itag;
        fifo_wptr <= 3'(fifo_wptr + 1);
      end
      if (fifo_ren) begin
        fifo_rptr <= 3'(fifo_rptr + 1);
      end
    end
  end
  assign nice_req_valid = ~nice_i_xs_off & nice_i_valid & nice_o_ready;
  assign nice_req_instr = nice_i_instr;
  assign nice_req_rs1 = nice_i_rs1;
  assign nice_req_rs2 = nice_i_rs2;
  assign nice_i_ready = nice_req_ready_pos & nice_o_ready;
  assign nice_o_valid = nice_i_valid & nice_req_ready_pos;
  assign nice_o_longpipe = ~nice_i_xs_off;
  assign nice_o_itag_valid = fifo_o_vld & nice_rsp_multicyc_valid;
  assign nice_o_itag = fifo_o_dat;
  assign nice_rsp_multicyc_ready = nice_o_itag_ready & fifo_o_vld;

endmodule

