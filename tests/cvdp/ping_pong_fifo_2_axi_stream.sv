module ping_pong_fifo_2_axi_stream #(
  parameter int DATA_WIDTH = 24,
  parameter int STROBE_WIDTH = 3,
  parameter int USE_KEEP = 0,
  parameter int USER_IN_DATA = 1
) (
  input logic rst,
  input logic i_block_fifo_rdy,
  output logic o_block_fifo_act = 0,
  input logic [24-1:0] i_block_fifo_size,
  input logic [DATA_WIDTH + 1-1:0] i_block_fifo_data,
  output logic o_block_fifo_stb = 0,
  input logic [4-1:0] i_axi_user,
  input logic i_axi_clk,
  output logic [4-1:0] o_axi_user = 0,
  input logic i_axi_ready,
  output logic [DATA_WIDTH-1:0] o_axi_data = 0,
  output logic o_axi_last = 0,
  output logic o_axi_valid = 0
);

  logic [DATA_WIDTH-1:0] fifo_data_buffer = 0;
  logic fifo_valid_buffer = 0;
  logic fifo_last_buffer = 0;
  logic [24-1:0] read_count = 0;
  logic [24-1:0] block_size = 0;
  logic stb_prev = 0;
  logic act_prev = 0;
  always_ff @(posedge i_axi_clk or posedge rst) begin
    if (rst) begin
      act_prev <= 0;
      block_size <= 0;
      fifo_data_buffer <= 0;
      fifo_last_buffer <= 0;
      fifo_valid_buffer <= 0;
      o_axi_data <= 0;
      o_axi_last <= 0;
      o_axi_user <= 0;
      o_axi_valid <= 0;
      o_block_fifo_act <= 0;
      o_block_fifo_stb <= 0;
      read_count <= 0;
      stb_prev <= 0;
    end else begin
      stb_prev <= o_block_fifo_stb;
      act_prev <= o_block_fifo_act;
      if (~o_block_fifo_act & i_block_fifo_rdy & ~act_prev) begin
        o_block_fifo_act <= 1'b1;
        read_count <= 0;
        block_size <= i_block_fifo_size;
        o_block_fifo_stb <= 1'b1;
      end else if (o_block_fifo_act) begin
        if (stb_prev) begin
          fifo_data_buffer <= i_block_fifo_data[DATA_WIDTH - 1:0];
          fifo_valid_buffer <= 1'b1;
          if (24'(read_count + 1) >= block_size) begin
            fifo_last_buffer <= 1'b1;
            o_block_fifo_stb <= 1'b0;
            o_block_fifo_act <= 1'b0;
          end else begin
            fifo_last_buffer <= 1'b0;
            read_count <= 24'(read_count + 1);
          end
        end
      end
      if (fifo_valid_buffer) begin
        if (~o_axi_valid | i_axi_ready) begin
          o_axi_data <= fifo_data_buffer;
          o_axi_last <= fifo_last_buffer;
          o_axi_valid <= 1'b1;
          o_axi_user <= i_axi_user;
          fifo_valid_buffer <= 1'b0;
        end
      end else if (o_axi_valid & i_axi_ready) begin
        o_axi_valid <= 1'b0;
      end
    end
  end

endmodule

