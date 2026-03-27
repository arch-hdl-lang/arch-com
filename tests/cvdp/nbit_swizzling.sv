module nbit_swizzling #(
  parameter int DATA_WIDTH = 64
) (
  input logic [DATA_WIDTH-1:0] data_in,
  input logic [2-1:0] sel,
  output logic [DATA_WIDTH-1:0] data_out
);

  logic [DATA_WIDTH-1:0] rev_full;
  logic [DATA_WIDTH-1:0] rev_half;
  logic [DATA_WIDTH-1:0] rev_quarter;
  logic [DATA_WIDTH-1:0] rev_eighth;
  always_comb begin
    for (int i = 0; i <= DATA_WIDTH - 1; i++) begin
      rev_full[i] = data_in[DATA_WIDTH - 1 - i];
    end
    for (int i = 0; i <= DATA_WIDTH - 1; i++) begin
      rev_half[i] = data_in[i / (DATA_WIDTH / 2) * (DATA_WIDTH / 2) + (DATA_WIDTH / 2 - 1) - i % (DATA_WIDTH / 2)];
    end
    for (int i = 0; i <= DATA_WIDTH - 1; i++) begin
      rev_quarter[i] = data_in[i / (DATA_WIDTH / 4) * (DATA_WIDTH / 4) + (DATA_WIDTH / 4 - 1) - i % (DATA_WIDTH / 4)];
    end
    for (int i = 0; i <= DATA_WIDTH - 1; i++) begin
      rev_eighth[i] = data_in[i / (DATA_WIDTH / 8) * (DATA_WIDTH / 8) + (DATA_WIDTH / 8 - 1) - i % (DATA_WIDTH / 8)];
    end
    if (sel == 2'd0) begin
      data_out = rev_full;
    end else if (sel == 2'd1) begin
      data_out = rev_half;
    end else if (sel == 2'd2) begin
      data_out = rev_quarter;
    end else if (sel == 2'd3) begin
      data_out = rev_eighth;
    end else begin
      data_out = data_in;
    end
  end

endmodule

// sel=0: reverse entire input
// sel=1: reverse two halves
// sel=2: reverse four quarters
// sel=3: reverse eight eighths
