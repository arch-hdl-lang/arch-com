module intra_block #(
  parameter int ROW_COL_WIDTH = 16,
  parameter int DATA_WIDTH = ROW_COL_WIDTH * ROW_COL_WIDTH
) (
  input logic [DATA_WIDTH-1:0] in_data,
  output logic [DATA_WIDTH-1:0] out_data
);

  // Pure combinational bit permutation.
  // For each output bit j, compute the source input bit index:
  //   row = j / ROW_COL_WIDTH
  //   if j < DATA_WIDTH/2:
  //     r_prime = (j - 2*row) % ROW_COL_WIDTH
  //     c_prime = (j - row)   % ROW_COL_WIDTH
  //   else:
  //     r_prime = (j - 2*row - 1) % ROW_COL_WIDTH
  //     c_prime = (j - row - 1)   % ROW_COL_WIDTH
  //   src = r_prime * ROW_COL_WIDTH + c_prime
  always_comb begin
    for (int j = 0; j <= DATA_WIDTH - 1; j++) begin
      if (j < DATA_WIDTH / 2) begin
        out_data[j] = in_data[(j - 2 * (j / ROW_COL_WIDTH)) % ROW_COL_WIDTH * ROW_COL_WIDTH + (j - j / ROW_COL_WIDTH) % ROW_COL_WIDTH];
      end else begin
        out_data[j] = in_data[(j - 2 * (j / ROW_COL_WIDTH) - 1) % ROW_COL_WIDTH * ROW_COL_WIDTH + (j - j / ROW_COL_WIDTH - 1) % ROW_COL_WIDTH];
      end
    end
  end

endmodule

module inter_block #(
  parameter int ROW_COL_WIDTH = 16,
  parameter int SUB_BLOCKS = 4,
  parameter int DATA_WIDTH = ROW_COL_WIDTH * ROW_COL_WIDTH,
  localparam int CHUNK = 8,
  localparam int OUT_CYCLES = DATA_WIDTH / CHUNK,
  localparam int NBW_SUB = $clog2(SUB_BLOCKS)
) (
  input logic clk,
  input logic rst_n,
  input logic i_valid,
  input logic [DATA_WIDTH-1:0] in_data,
  output logic [DATA_WIDTH-1:0] out_data,
  output logic [DATA_WIDTH-1:0] out_data_aux [SUB_BLOCKS-1:0],
  output logic start_intra,
  output logic [NBW_SUB-1:0] counter_sub_out
);

  logic [NBW_SUB-1:0] cnt_sub_blocks;
  logic start_latched;
  logic [6-1:0] start_pipe;
  logic [DATA_WIDTH-1:0] in_data_reg [SUB_BLOCKS-1:0];
  logic [DATA_WIDTH-1:0] out_data_aux_reg [SUB_BLOCKS-1:0];
  logic [DATA_WIDTH-1:0] out_data_reg;
  logic [NBW_SUB-1:0] counter_sub_out_reg;
  logic [DATA_WIDTH-1:0] intra_out [SUB_BLOCKS-1:0];
  logic [DATA_WIDTH-1:0] next_out_data_aux [SUB_BLOCKS-1:0];
  logic [NBW_SUB-1:0] next_counter_sub_out;
  logic next_start_head;
  logic [DATA_WIDTH-1:0] next_out_data_selected;
  intra_block #(.ROW_COL_WIDTH(ROW_COL_WIDTH)) ib0 (
    .in_data(in_data_reg[0]),
    .out_data(intra_out[0])
  );
  intra_block #(.ROW_COL_WIDTH(ROW_COL_WIDTH)) ib1 (
    .in_data(in_data_reg[1]),
    .out_data(intra_out[1])
  );
  intra_block #(.ROW_COL_WIDTH(ROW_COL_WIDTH)) ib2 (
    .in_data(in_data_reg[2]),
    .out_data(intra_out[2])
  );
  intra_block #(.ROW_COL_WIDTH(ROW_COL_WIDTH)) ib3 (
    .in_data(in_data_reg[3]),
    .out_data(intra_out[3])
  );
  always_comb begin
    next_start_head = start_latched || i_valid && cnt_sub_blocks == SUB_BLOCKS - 1;
    if (counter_sub_out_reg == SUB_BLOCKS - 1) begin
      next_counter_sub_out = 0;
    end else begin
      next_counter_sub_out = NBW_SUB'(counter_sub_out_reg + 1);
    end
    for (int b = 0; b <= SUB_BLOCKS - 1; b++) begin
      next_out_data_aux[b] = out_data_aux_reg[b];
    end
    if (start_pipe[4]) begin
      for (int b = 0; b <= SUB_BLOCKS - 1; b++) begin
        next_out_data_aux[b] = 0;
      end
      for (int i = 0; i <= OUT_CYCLES - 1; i++) begin
        next_out_data_aux[0][i * CHUNK +: CHUNK] = intra_out[i % SUB_BLOCKS][i * CHUNK +: CHUNK];
        next_out_data_aux[1][i * CHUNK +: CHUNK] = intra_out[i % SUB_BLOCKS][(i + 1) % OUT_CYCLES * CHUNK +: CHUNK];
        next_out_data_aux[2][i * CHUNK +: CHUNK] = intra_out[i % SUB_BLOCKS][(i + 2) % OUT_CYCLES * CHUNK +: CHUNK];
        next_out_data_aux[3][i * CHUNK +: CHUNK] = intra_out[i % SUB_BLOCKS][(i + 3) % OUT_CYCLES * CHUNK +: CHUNK];
      end
    end
    if (next_counter_sub_out == 0) begin
      next_out_data_selected = next_out_data_aux[0];
    end else if (next_counter_sub_out == 1) begin
      next_out_data_selected = next_out_data_aux[1];
    end else if (next_counter_sub_out == 2) begin
      next_out_data_selected = next_out_data_aux[2];
    end else begin
      next_out_data_selected = next_out_data_aux[3];
    end
    out_data = out_data_reg;
    start_intra = start_pipe[4];
    counter_sub_out = counter_sub_out_reg;
    for (int b = 0; b <= SUB_BLOCKS - 1; b++) begin
      out_data_aux[b] = out_data_aux_reg[b];
    end
  end
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      cnt_sub_blocks <= 0;
      counter_sub_out_reg <= 0;
      for (int __ri0 = 0; __ri0 < SUB_BLOCKS; __ri0++) begin
        in_data_reg[__ri0] <= 0;
      end
      for (int __ri0 = 0; __ri0 < SUB_BLOCKS; __ri0++) begin
        out_data_aux_reg[__ri0] <= 0;
      end
      out_data_reg <= 0;
      start_latched <= 0;
      start_pipe <= 0;
    end else begin
      if (i_valid) begin
        in_data_reg[cnt_sub_blocks] <= in_data;
        if (cnt_sub_blocks == SUB_BLOCKS - 1) begin
          cnt_sub_blocks <= 0;
          start_latched <= 1;
        end else begin
          cnt_sub_blocks <= NBW_SUB'(cnt_sub_blocks + 1);
        end
      end
      start_pipe[5] <= start_pipe[4];
      start_pipe[4] <= start_pipe[3];
      start_pipe[3] <= start_pipe[2];
      start_pipe[2] <= start_pipe[1];
      start_pipe[1] <= next_start_head;
      start_pipe[0] <= next_start_head;
      if (start_pipe[4]) begin
        for (int b = 0; b <= SUB_BLOCKS - 1; b++) begin
          out_data_aux_reg[b] <= next_out_data_aux[b];
        end
        counter_sub_out_reg <= next_counter_sub_out;
        out_data_reg <= next_out_data_selected;
      end
    end
  end

endmodule

