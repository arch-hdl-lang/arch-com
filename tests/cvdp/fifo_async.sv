module fifo_async #(
  parameter int DATA_WIDTH = 32,
  parameter int DEPTH = 8,
  parameter int ADDR_WIDTH = 3
) (
  input logic w_clk,
  input logic w_rst,
  input logic w_inc,
  input logic [DATA_WIDTH-1:0] w_data,
  input logic r_clk,
  input logic r_rst,
  input logic r_inc,
  output logic w_full,
  output logic r_empty,
  output logic [DATA_WIDTH-1:0] r_data
);

  // Memory array
  logic [DEPTH-1:0] [DATA_WIDTH-1:0] mem;
  // Write pointer binary
  logic [ADDR_WIDTH + 1-1:0] w_bin;
  // Read pointer binary
  logic [ADDR_WIDTH + 1-1:0] r_bin;
  // Synchronized pointers (binary, after gray sync)
  logic [ADDR_WIDTH + 1-1:0] r_bin_synced;
  logic [ADDR_WIDTH + 1-1:0] w_bin_synced;
  // Synchronize read pointer into write domain (gray CDC)
  r2w_sync r2w (
    .src_clk(r_clk),
    .dst_clk(w_clk),
    .rst(w_rst),
    .data_in(r_bin),
    .data_out(r_bin_synced)
  );
  // Synchronize write pointer into read domain (gray CDC)
  w2r_sync w2r (
    .src_clk(w_clk),
    .dst_clk(r_clk),
    .rst(r_rst),
    .data_in(w_bin),
    .data_out(w_bin_synced)
  );
  // Next values
  logic [ADDR_WIDTH + 1-1:0] w_bin_next;
  logic [ADDR_WIDTH + 1-1:0] r_bin_next;
  logic w_full_next;
  logic r_empty_next;
  logic [ADDR_WIDTH-1:0] w_addr;
  logic [ADDR_WIDTH-1:0] r_addr;
  // Write/read addresses are lower bits of binary pointer
  assign w_addr = w_bin[ADDR_WIDTH - 1:0];
  assign r_addr = r_bin[ADDR_WIDTH - 1:0];
  // Next binary pointers
  always_comb begin
    if (w_inc & ~w_full) begin
      w_bin_next = (ADDR_WIDTH + 1)'(w_bin + 1);
    end else begin
      w_bin_next = w_bin;
    end
  end
  always_comb begin
    if (r_inc & ~r_empty) begin
      r_bin_next = (ADDR_WIDTH + 1)'(r_bin + 1);
    end else begin
      r_bin_next = r_bin;
    end
  end
  // Full: MSB different, rest same (binary comparison)
  assign w_full_next = w_bin_next[ADDR_WIDTH] != r_bin_synced[ADDR_WIDTH] & w_bin_next[ADDR_WIDTH - 1:0] == r_bin_synced[ADDR_WIDTH - 1:0];
  // Empty: pointers identical (binary comparison)
  assign r_empty_next = r_bin_next == w_bin_synced;
  // Async read
  assign r_data = mem[r_addr];
  // Write domain sequential logic
  always_ff @(posedge w_clk) begin
    if (w_rst) begin
      for (int __ri0 = 0; __ri0 < DEPTH; __ri0++) begin
        mem[__ri0] <= 0;
      end
      w_bin <= 0;
      w_full <= 1'b0;
    end else begin
      w_bin <= w_bin_next;
      w_full <= w_full_next;
      if (w_inc & ~w_full) begin
        mem[w_addr] <= w_data;
      end
    end
  end
  // Read domain sequential logic
  always_ff @(posedge r_clk) begin
    if (r_rst) begin
      r_bin <= 0;
      r_empty <= 1'b1;
    end else begin
      r_bin <= r_bin_next;
      r_empty <= r_empty_next;
    end
  end

endmodule

