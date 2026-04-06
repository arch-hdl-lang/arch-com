// domain SrcDomain
//   freq_mhz: 100

// domain DstDomain
//   freq_mhz: 250

module BusSync #(
  parameter int STAGES = 2
) (
  input logic src_clk,
  input logic dst_clk,
  input logic rst,
  input logic [32-1:0] data_in,
  output logic [32-1:0] data_out
);

  // Handshake synchronizer (2 stages, src_clk → dst_clk)
  logic [32-1:0] data_reg;
  logic req_src, ack_src;
  logic req_sync [0:STAGES-1];  // req synchronized to dst_clk
  logic ack_sync [0:STAGES-1];  // ack synchronized to src_clk
  logic ack_dst;
  
  // Source domain (src_clk): latch data, manage req/ack
  always_ff @(posedge src_clk) begin
    if (rst) begin
      req_src <= 1'b0;
      data_reg <= '0;
    end else if (data_in !== data_reg && req_src == ack_src) begin
      data_reg <= data_in;
      req_src <= ~req_src;
    end
  end
  
  // Synchronize req into dst_clk
  always_ff @(posedge dst_clk) begin
    if (rst) begin
      for (int i = 0; i < STAGES; i++) req_sync[i] <= 1'b0;
      ack_dst <= 1'b0;
    end else begin
      req_sync[0] <= req_src;
      for (int i = 1; i < STAGES; i++) req_sync[i] <= req_sync[i-1];
      ack_dst <= req_sync[STAGES-1];
    end
  end
  
  // Synchronize ack back into src_clk
  always_ff @(posedge src_clk) begin
    if (rst) begin
      for (int i = 0; i < STAGES; i++) ack_sync[i] <= 1'b0;
    end else begin
      ack_sync[0] <= ack_dst;
      for (int i = 1; i < STAGES; i++) ack_sync[i] <= ack_sync[i-1];
    end
  end
  
  assign ack_src = ack_sync[STAGES-1];
  assign data_out = data_reg;

endmodule

