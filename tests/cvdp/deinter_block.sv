module deinter_block #(
  parameter int ROW_COL_WIDTH = 16,
  parameter int SUB_BLOCKS = 4,
  parameter int DATA_WIDTH = ROW_COL_WIDTH * ROW_COL_WIDTH,
  parameter int OUT_DATA_WIDTH = 16,
  parameter int WAIT_CYCLES = 4,
  localparam int CHUNK = 8,
  localparam int NBW_CNT = $clog2(SUB_BLOCKS) + 1,
  localparam int OUT_CYCLES = 32,
  localparam int N_CYCLES = (SUB_BLOCKS * DATA_WIDTH) / OUT_DATA_WIDTH,
  localparam int NBW_CNT_OUT = $clog2(N_CYCLES),
  localparam int DELAY_LEN = WAIT_CYCLES + 1
) (
  input logic clk,
  input logic rst_n,
  input logic i_valid,
  input logic [DATA_WIDTH-1:0] in_data,
  output logic [OUT_DATA_WIDTH-1:0] out_data
);

  // --- Input registration ---
  logic [NBW_CNT-1:0] cnt_sub_blocks;
  logic start_intra;
  logic [SUB_BLOCKS-1:0] [DATA_WIDTH-1:0] in_data_reg;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      cnt_sub_blocks <= 0;
      for (int __ri0 = 0; __ri0 < SUB_BLOCKS; __ri0++) begin
        in_data_reg[__ri0] <= 0;
      end
      start_intra <= 0;
    end else begin
      if (i_valid) begin
        in_data_reg[$clog2(SUB_BLOCKS)'(cnt_sub_blocks)] <= in_data;
        if (cnt_sub_blocks == SUB_BLOCKS - 1) begin
          cnt_sub_blocks <= 0;
          start_intra <= 1;
        end else begin
          cnt_sub_blocks <= NBW_CNT'(cnt_sub_blocks + 1);
          start_intra <= 0;
        end
      end
    end
  end
  // --- Register intra-block data (identity transform) ---
  logic [SUB_BLOCKS-1:0] [DATA_WIDTH-1:0] out_data_intra_block_reg;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      for (int __ri0 = 0; __ri0 < SUB_BLOCKS; __ri0++) begin
        out_data_intra_block_reg[__ri0] <= 0;
      end
    end else begin
      if (start_intra) begin
        for (int i = 0; i <= SUB_BLOCKS - 1; i++) begin
          out_data_intra_block_reg[i] <= in_data_reg[i];
        end
      end
    end
  end
  // --- Delay chain ---
  logic [DELAY_LEN-1:0] start_intra_ff;
  logic enable_output;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      enable_output <= 0;
      start_intra_ff <= 0;
    end else begin
      enable_output <= start_intra_ff[DELAY_LEN - 1];
      start_intra_ff <= {start_intra_ff[DELAY_LEN - 2:0], start_intra};
    end
  end
  // --- Rearrange data into out_data_aux ---
  logic [SUB_BLOCKS-1:0] [DATA_WIDTH-1:0] out_data_aux;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      for (int __ri0 = 0; __ri0 < SUB_BLOCKS; __ri0++) begin
        out_data_aux[__ri0] <= 0;
      end
    end else begin
      if (start_intra) begin
        for (int i = 0; i <= OUT_CYCLES - 1; i++) begin
          out_data_aux[0][i * CHUNK +: CHUNK] <= out_data_intra_block_reg[i % 4][i * CHUNK +: CHUNK];
          out_data_aux[1][i * CHUNK +: CHUNK] <= out_data_intra_block_reg[i % 4][((i + 1) % OUT_CYCLES) * CHUNK +: CHUNK];
          out_data_aux[2][i * CHUNK +: CHUNK] <= out_data_intra_block_reg[i % 4][((i + 2) % OUT_CYCLES) * CHUNK +: CHUNK];
          out_data_aux[3][i * CHUNK +: CHUNK] <= out_data_intra_block_reg[i % 4][((i + 3) % OUT_CYCLES) * CHUNK +: CHUNK];
        end
      end
    end
  end
  // --- Output logic ---
  logic [1:0] cnt_sub_out;
  logic [NBW_CNT_OUT-1:0] cnt_output;
  // Combinational: select the right chunk from out_data_aux
  logic [DATA_WIDTH-1:0] selected_aux;
  assign selected_aux = out_data_aux[cnt_sub_out];
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      cnt_output <= 0;
      cnt_sub_out <= 0;
      out_data <= 0;
    end else begin
      if (enable_output || cnt_output > 0) begin
        cnt_sub_out <= 2'(cnt_sub_out + 1);
        cnt_output <= NBW_CNT_OUT'(cnt_output + 1);
        out_data <= selected_aux[(cnt_output % (DATA_WIDTH / OUT_DATA_WIDTH)) * OUT_DATA_WIDTH +: OUT_DATA_WIDTH];
      end
    end
  end

endmodule

