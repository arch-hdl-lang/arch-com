module WToRGraySync #(
  parameter int STAGES = 2,
  parameter int WIDTH = 4
) (
  input logic src_clk,
  input logic dst_clk,
  input logic rst,
  input logic [WIDTH-1:0] data_in,
  output logic [WIDTH-1:0] data_out
);

  // Gray-code synchronizer (2 stages, src_clk → dst_clk)
  logic [WIDTH-1:0] bin_to_gray;
  logic [WIDTH-1:0] gray_chain [0:STAGES-1];
  logic [WIDTH-1:0] gray_to_bin;
  
  assign bin_to_gray = data_in ^ (data_in >> 1);
  
  always_ff @(posedge dst_clk or posedge rst) begin
    if (rst) begin
      for (int i = 0; i < STAGES; i++) gray_chain[i] <= '0;
    end else begin
      gray_chain[0] <= bin_to_gray;
      for (int i = 1; i < STAGES; i++) gray_chain[i] <= gray_chain[i-1];
    end
  end
  
  // Gray-to-binary decode (prefix XOR — no self-reference)
  always_comb begin
    gray_to_bin = gray_chain[STAGES-1];
    for (int i = 1; i < $bits(logic [WIDTH-1:0]); i++)
      gray_to_bin ^= gray_chain[STAGES-1] >> i;
  end
  
  assign data_out = gray_to_bin;

endmodule

