// domain SrcDomain
//   freq_mhz: 100

// domain DstDomain
//   freq_mhz: 200

module EventSync #(
  parameter int STAGES = 2
) (
  input logic src_clk,
  input logic dst_clk,
  input logic rst,
  input logic data_in,
  output logic data_out
);

  // Pulse synchronizer: src_clk → dst_clk
  // Source: pulse → toggle; Destination: sync toggle → edge detect → pulse
  logic toggle_src;
  logic sync_chain [0:STAGES-1];
  logic pulse_dst;
  
  always_ff @(posedge src_clk) begin
    if (rst) toggle_src <= 1'b0;
    else if (data_in) toggle_src <= ~toggle_src;
  end
  
  always_ff @(posedge dst_clk) begin
    if (rst) begin
      for (int i = 0; i < STAGES; i++) sync_chain[i] <= 1'b0;
    end else begin
      sync_chain[0] <= toggle_src;
      for (int i = 1; i < STAGES; i++) sync_chain[i] <= sync_chain[i-1];
    end
  end
  
  assign data_out = sync_chain[STAGES-1] ^ sync_chain[STAGES-2];

endmodule

