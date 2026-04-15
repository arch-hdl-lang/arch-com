module tx_block_parity #(
  parameter int DATA_WIDTH = 8
) (
  input logic clk,
  input logic rst_n,
  input logic [DATA_WIDTH-1:0] data_in,
  input logic [2:0] sel,
  input logic load,
  output logic serial_out,
  output logic parity
);

  logic [DATA_WIDTH-1:0] shift_reg;
  logic parity_reg;
  assign serial_out = shift_reg[0:0] == 1;
  assign parity = parity_reg;
  // Compute parity over data_in[sel-1:0] combinationally
  // Since sel is dynamic, we XOR all DATA_WIDTH bits but masked by index < sel
  logic par_comb;
  always_comb begin
    par_comb = 1'b0;
    for (int i = 0; i <= DATA_WIDTH - 1; i++) begin
      if (3'($unsigned(i)) < sel) begin
        par_comb = par_comb ^ (data_in[i +: 1] == 1);
      end
    end
  end
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      parity_reg <= 1'b0;
      shift_reg <= 0;
    end else begin
      if (load) begin
        shift_reg <= data_in;
        parity_reg <= par_comb;
      end else begin
        shift_reg <= shift_reg >> 1;
      end
    end
  end

endmodule

