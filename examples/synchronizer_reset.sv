// domain SrcDomain
//   freq_mhz: 100

// domain DstDomain
//   freq_mhz: 200

module RstSync #(
  parameter int STAGES = 2
) (
  input logic src_clk,
  input logic dst_clk,
  input logic data_in,
  output logic data_out
);

  // Reset synchronizer: async assert, sync deassert on dst_clk
  logic sync_chain [0:STAGES-1];
  
  always_ff @(posedge dst_clk or posedge data_in) begin
    if (data_in) begin
      for (int i = 0; i < STAGES; i++) sync_chain[i] <= 1'b1;
    end else begin
      sync_chain[0] <= 1'b0;
      for (int i = 1; i < STAGES; i++) sync_chain[i] <= sync_chain[i-1];
    end
  end
  
  assign data_out = sync_chain[STAGES-1];

endmodule

