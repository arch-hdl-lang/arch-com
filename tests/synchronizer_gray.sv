// domain SrcDomain
//   freq_mhz: 100

// domain DstDomain
//   freq_mhz: 133

module PtrSync #(
  parameter int STAGES = 2
) (
  input logic src_clk,
  input logic dst_clk,
  input logic rst,
  input logic [4-1:0] data_in,
  output logic [4-1:0] data_out
);

  // Gray-code synchronizer (2 stages, src_clk → dst_clk)
  logic [4-1:0] bin_to_gray;
  logic [4-1:0] gray_chain [0:STAGES-1];
  logic [4-1:0] gray_to_bin;
  
  assign bin_to_gray = data_in ^ (data_in >> 1);
  
  always_ff @(posedge dst_clk) begin
    if (rst) begin
      for (int i = 0; i < STAGES; i++) gray_chain[i] <= '0;
    end else begin
      gray_chain[0] <= bin_to_gray;
      for (int i = 1; i < STAGES; i++) gray_chain[i] <= gray_chain[i-1];
    end
  end
  
  // Gray-to-binary decode
  always_comb begin
    gray_to_bin[$bits(logic [4-1:0])-1] = gray_chain[STAGES-1][$bits(logic [4-1:0])-1];
    for (int i = $bits(logic [4-1:0])-2; i >= 0; i--)
      gray_to_bin[i] = gray_chain[STAGES-1][i] ^ gray_to_bin[i+1];
  end
  
  assign data_out = gray_to_bin;

endmodule

