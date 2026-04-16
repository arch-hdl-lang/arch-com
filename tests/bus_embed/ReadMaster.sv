package PkgTest;
endpackage

// A simple read master using the composed BusAxi4Read
module ReadMaster #(
  parameter int ADDR_W = 32,
  parameter int DATA_W = 32,
  parameter int ID_W = 4
) (
  input logic clk,
  input logic rst,
  output logic axi_rd_ar_valid,
  input logic axi_rd_ar_ready,
  output logic [ADDR_W-1:0] axi_rd_ar_addr,
  output logic [ID_W-1:0] axi_rd_ar_id,
  output logic [7:0] axi_rd_ar_len,
  input logic axi_rd_r_valid,
  output logic axi_rd_r_ready,
  input logic [DATA_W-1:0] axi_rd_r_data,
  input logic [ID_W-1:0] axi_rd_r_id,
  input logic axi_rd_r_last,
  input logic start_i,
  input logic [ADDR_W-1:0] addr_i,
  output logic [DATA_W-1:0] data_o,
  output logic data_vld_o
);

  // Read-only AXI master — uses the embed-composed bus
  // Control
  logic active_q;
  // Drive AR channel using the prefixed signal names (ar_valid, ar_addr, etc.)
  assign axi_rd_ar_valid = start_i && !active_q;
  assign axi_rd_ar_addr = addr_i;
  assign axi_rd_ar_id = 0;
  assign axi_rd_ar_len = 0;
  // Accept R channel
  assign axi_rd_r_ready = 1'b1;
  assign data_o = axi_rd_r_data;
  assign data_vld_o = axi_rd_r_valid;
  always_ff @(posedge clk) begin
    if (rst) begin
      active_q <= 1'b0;
    end else begin
      if (start_i && !active_q) begin
        active_q <= 1'b1;
      end else if (axi_rd_r_valid && axi_rd_r_last) begin
        active_q <= 1'b0;
      end
    end
  end

endmodule

// Minimal AXI address channel bus
// AXI R data channel bus
// AXI W data channel bus
// Composed read-only AXI bus using embed
// Composed write-only AXI bus using embed
// Composed full AXI bus: re-embeds the same primitives
