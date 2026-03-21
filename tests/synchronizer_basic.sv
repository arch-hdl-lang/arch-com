// domain FastDomain
//   freq_mhz: 200

// domain SlowDomain
//   freq_mhz: 50

module FlagSync #(
  parameter int STAGES = 2
) (
  input logic src_clk,
  input logic dst_clk,
  input logic rst,
  input logic data_in,
  output logic data_out
);

  // 2-stage synchronizer chain (destination clock: dst_clk)
  logic sync_chain [0:STAGES-1];
  
  always_ff @(posedge dst_clk or posedge rst) begin
    if (rst) begin
      for (int i = 0; i < STAGES; i++) sync_chain[i] <= '0;
    end else begin
      sync_chain[0] <= data_in;
      for (int i = 1; i < STAGES; i++) sync_chain[i] <= sync_chain[i-1];
    end
  end
  
  assign data_out = sync_chain[STAGES-1];

endmodule

