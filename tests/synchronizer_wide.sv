// domain SrcDomain
//   freq_mhz: 100

// domain DstDomain
//   freq_mhz: 133

module DataSync #(
  parameter int STAGES = 3
) (
  input logic src_clk,
  input logic dst_clk,
  input logic rst,
  input logic [8-1:0] data_in,
  output logic [8-1:0] data_out
);

  // 3-stage FF synchronizer chain (destination clock: dst_clk)
  logic [8-1:0] sync_chain [0:STAGES-1];
  
  always_ff @(posedge dst_clk) begin
    if (rst) begin
      for (int i = 0; i < STAGES; i++) sync_chain[i] <= '0;
    end else begin
      sync_chain[0] <= data_in;
      for (int i = 1; i < STAGES; i++) sync_chain[i] <= sync_chain[i-1];
    end
  end
  
  assign data_out = sync_chain[STAGES-1];

endmodule

