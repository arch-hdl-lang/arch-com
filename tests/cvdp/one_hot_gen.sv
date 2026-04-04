module one_hot_gen #(
  parameter int NS_A = 8,
  parameter int NS_B = 4
) (
  input logic clk,
  input logic rst_async_n,
  input logic [2-1:0] i_config,
  input logic i_start,
  input logic o_ready,
  output logic [NS_A + NS_B-1:0] o_address_one_hot
);

  logic [NS_A + NS_B-1:0] addr_r;
  logic [4-1:0] idx;
  assign o_address_one_hot = addr_r;
  always_ff @(posedge clk or negedge rst_async_n) begin
    if ((!rst_async_n)) begin
      addr_r <= 0;
      idx <= 0;
    end else begin
      if (i_start & o_ready) begin
        addr_r <= (NS_A + NS_B)'($unsigned(1)) << idx;
        if (idx == 4'(NS_A + NS_B - 1)) begin
          idx <= 0;
        end else begin
          idx <= 4'(idx + 1);
        end
      end else if (~i_start) begin
        idx <= 0;
        addr_r <= 0;
      end
    end
  end

endmodule

