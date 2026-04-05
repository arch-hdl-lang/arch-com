// Write buffer merge: merges BUFFER_DEPTH input writes into one wider output write.
// NOTE: ARCH generate-if cannot contain reg/seq, so this covers BUFFER_DEPTH > 1.
// The .sv file includes both generate branches (BUFFER_DEPTH > 1 and == 1).
module write_buffer_merge #(
  parameter int INPUT_DATA_WIDTH = 32,
  parameter int INPUT_ADDR_WIDTH = 16,
  parameter int BUFFER_DEPTH = 8,
  parameter int CNT_W = $clog2(BUFFER_DEPTH + 1),
  parameter int OUTPUT_DATA_WIDTH = INPUT_DATA_WIDTH * BUFFER_DEPTH,
  parameter int OUTPUT_ADDR_WIDTH = INPUT_ADDR_WIDTH - $clog2(BUFFER_DEPTH)
) (
  input logic clk,
  input logic srst,
  input logic wr_en_in,
  input logic [INPUT_ADDR_WIDTH-1:0] wr_addr_in,
  input logic [INPUT_DATA_WIDTH-1:0] wr_data_in,
  output logic wr_en_out,
  output logic [OUTPUT_ADDR_WIDTH-1:0] wr_addr_out,
  output logic [OUTPUT_DATA_WIDTH-1:0] wr_data_out
);

  logic [CNT_W-1:0] write_cnt;
  logic [OUTPUT_ADDR_WIDTH-1:0] base_addr;
  logic [OUTPUT_DATA_WIDTH-1:0] merged_data;
  logic write_complete;
  logic [OUTPUT_ADDR_WIDTH-1:0] out_addr_pending;
  logic [OUTPUT_DATA_WIDTH-1:0] out_data_pending;
  // Write count logic
  always_ff @(posedge clk) begin
    if (srst) begin
      write_cnt <= 0;
    end else begin
      if (wr_en_in) begin
        if (write_cnt == CNT_W'(BUFFER_DEPTH - 1)) begin
          write_cnt <= 0;
        end else begin
          write_cnt <= CNT_W'(write_cnt + 1);
        end
      end
    end
  end
  // Base address logic: capture MSBs of address on first write
  always_ff @(posedge clk) begin
    if (srst) begin
      base_addr <= 0;
    end else begin
      if (wr_en_in & write_cnt == 0) begin
        base_addr <= wr_addr_in[INPUT_ADDR_WIDTH - 1:$clog2(BUFFER_DEPTH)];
      end
    end
  end
  // Merged data logic: shift right and append new data at MSB
  always_ff @(posedge clk) begin
    if (srst) begin
      merged_data <= 0;
    end else begin
      if (wr_en_in) begin
        merged_data <= merged_data >> INPUT_DATA_WIDTH | OUTPUT_DATA_WIDTH'($unsigned(wr_data_in)) << OUTPUT_DATA_WIDTH - INPUT_DATA_WIDTH;
      end
    end
  end
  // Write completion: buffer full
  always_ff @(posedge clk) begin
    if (srst) begin
      out_addr_pending <= 0;
      out_data_pending <= 0;
      write_complete <= 1'b0;
    end else begin
      if (write_cnt == CNT_W'(BUFFER_DEPTH - 1) & wr_en_in) begin
        write_complete <= 1'b1;
        out_addr_pending <= wr_addr_in[INPUT_ADDR_WIDTH - 1:$clog2(BUFFER_DEPTH)];
        out_data_pending <= merged_data >> INPUT_DATA_WIDTH | OUTPUT_DATA_WIDTH'($unsigned(wr_data_in)) << OUTPUT_DATA_WIDTH - INPUT_DATA_WIDTH;
      end else begin
        write_complete <= 1'b0;
      end
    end
  end
  // Output logic: depth==1 is pass-through (1-cycle), depth>1 emits merged writes.
  always_ff @(posedge clk) begin
    if (srst) begin
      wr_addr_out <= 0;
      wr_data_out <= 0;
      wr_en_out <= 1'b0;
    end else begin
      if (BUFFER_DEPTH == 1) begin
        if (wr_en_in) begin
          wr_en_out <= 1'b1;
          wr_addr_out <= wr_addr_in[INPUT_ADDR_WIDTH - 1:$clog2(BUFFER_DEPTH)];
          wr_data_out <= OUTPUT_DATA_WIDTH'($unsigned(wr_data_in));
        end else begin
          wr_en_out <= 1'b0;
        end
      end else if (write_complete) begin
        wr_en_out <= 1'b1;
        wr_addr_out <= out_addr_pending;
        wr_data_out <= out_data_pending;
      end else begin
        wr_en_out <= 1'b0;
      end
    end
  end

endmodule

