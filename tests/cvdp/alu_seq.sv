module alu_seq #(
  parameter int p_key = 'hAA
) (
  input logic i_clk,
  input logic i_rst_b,
  input logic [4-1:0] i_operand_a,
  input logic [4-1:0] i_operand_b,
  input logic [3-1:0] i_opcode,
  input logic [8-1:0] i_key_in,
  output logic [8-1:0] o_result
);

  always_ff @(posedge i_clk or negedge i_rst_b) begin
    if ((!i_rst_b)) begin
      o_result <= 0;
    end else begin
      if (i_key_in != p_key) begin
        o_result <= 0;
      end else if (i_opcode == 0) begin
        o_result <= 8'(8'($unsigned(i_operand_a)) + 8'($unsigned(i_operand_b)));
      end else if (i_opcode == 1) begin
        o_result <= 8'(8'($unsigned(i_operand_a)) - 8'($unsigned(i_operand_b)));
      end else if (i_opcode == 2) begin
        o_result <= 8'(8'($unsigned(i_operand_a)) * 8'($unsigned(i_operand_b)));
      end else if (i_opcode == 3) begin
        o_result <= 8'($unsigned(i_operand_a & i_operand_b));
      end else if (i_opcode == 4) begin
        o_result <= 8'($unsigned(i_operand_a | i_operand_b));
      end else if (i_opcode == 5) begin
        o_result <= 8'($unsigned(~i_operand_a));
      end else if (i_opcode == 6) begin
        o_result <= 8'($unsigned(i_operand_a ^ i_operand_b));
      end else begin
        o_result <= 8'($unsigned(~(i_operand_a ^ i_operand_b)));
      end
    end
  end

endmodule

