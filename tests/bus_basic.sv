module Master (
  input logic clk,
  input logic rst,
  output logic axi_aw_valid,
  input logic axi_aw_ready,
  output logic [32-1:0] axi_aw_addr,
  output logic axi_w_valid,
  input logic axi_w_ready,
  output logic [64-1:0] axi_w_data,
  input logic axi_b_valid,
  output logic axi_b_ready,
  input logic [2-1:0] axi_b_resp
);

  logic [32-1:0] addr_r;
  logic [64-1:0] data_r;
  assign axi_aw_valid = 1;
  assign axi_aw_addr = addr_r;
  assign axi_w_valid = 1;
  assign axi_w_data = data_r;
  assign axi_b_ready = 1;

endmodule

module Slave (
  input logic clk,
  input logic rst,
  input logic axi_aw_valid,
  output logic axi_aw_ready,
  input logic [32-1:0] axi_aw_addr,
  input logic axi_w_valid,
  output logic axi_w_ready,
  input logic [64-1:0] axi_w_data,
  output logic axi_b_valid,
  input logic axi_b_ready,
  output logic [2-1:0] axi_b_resp
);

  assign axi_aw_ready = 1;
  assign axi_w_ready = 1;
  assign axi_b_valid = 1;
  assign axi_b_resp = 0;

endmodule

module Top (
  input logic clk,
  input logic rst
);

  logic aw_valid_w;
  logic aw_ready_w;
  logic [32-1:0] aw_addr_w;
  logic w_valid_w;
  logic w_ready_w;
  logic [64-1:0] w_data_w;
  logic b_valid_w;
  logic b_ready_w;
  logic [2-1:0] b_resp_w;
  Master m (
    .clk(clk),
    .rst(rst),
    .axi_aw_valid(aw_valid_w),
    .axi_aw_ready(aw_ready_w),
    .axi_aw_addr(aw_addr_w),
    .axi_w_valid(w_valid_w),
    .axi_w_ready(w_ready_w),
    .axi_w_data(w_data_w),
    .axi_b_valid(b_valid_w),
    .axi_b_ready(b_ready_w),
    .axi_b_resp(b_resp_w)
  );
  Slave s (
    .clk(clk),
    .rst(rst),
    .axi_aw_valid(aw_valid_w),
    .axi_aw_ready(aw_ready_w),
    .axi_aw_addr(aw_addr_w),
    .axi_w_valid(w_valid_w),
    .axi_w_ready(w_ready_w),
    .axi_w_data(w_data_w),
    .axi_b_valid(b_valid_w),
    .axi_b_ready(b_ready_w),
    .axi_b_resp(b_resp_w)
  );
  assign aw_valid_w = 0;
  assign aw_ready_w = 0;
  assign aw_addr_w = 0;
  assign w_valid_w = 0;
  assign w_ready_w = 0;
  assign w_data_w = 0;
  assign b_valid_w = 0;
  assign b_ready_w = 0;
  assign b_resp_w = 0;

endmodule

