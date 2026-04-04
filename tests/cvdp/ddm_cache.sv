module ddm_cache (
  input logic clk,
  input logic rst_n,
  input logic [32-1:0] cpu_addr,
  input logic [32-1:0] cpu_dout,
  input logic cpu_strobe,
  input logic cpu_rw,
  input logic uncached,
  input logic [32-1:0] mem_dout,
  input logic mem_ready,
  output logic [32-1:0] cpu_din,
  output logic [32-1:0] mem_din,
  output logic cpu_ready,
  output logic mem_strobe,
  output logic mem_rw,
  output logic [32-1:0] mem_addr,
  output logic cache_hit,
  output logic cache_miss,
  output logic [32-1:0] d_data_dout
);

  // 64-entry direct-mapped cache
  // index = cpu_addr[7:2] (6 bits), tag = cpu_addr[31:8] (24 bits)
  logic d_valid [64-1:0];
  logic [24-1:0] d_tags [64-1:0];
  logic [32-1:0] d_data [64-1:0];
  logic [6-1:0] idx;
  logic [24-1:0] tag;
  logic cache_hit_w;
  logic cache_miss_w;
  logic cache_write;
  assign idx = cpu_addr[7:2];
  assign tag = cpu_addr[31:8];
  assign cache_hit_w = cpu_strobe & d_valid[idx] & d_tags[idx] == tag;
  assign cache_miss_w = cpu_strobe & ~(d_valid[idx] & d_tags[idx] == tag);
  assign cache_write = ~uncached & (cpu_rw | cache_miss_w & mem_ready);
  assign cache_hit = cache_hit_w;
  assign cache_miss = cache_miss_w;
  assign cpu_din = d_data[idx];
  assign d_data_dout = d_data[idx];
  assign mem_din = cpu_dout;
  assign mem_addr = cpu_addr;
  assign mem_rw = cpu_rw;
  assign mem_strobe = cpu_strobe & ~uncached & (cpu_rw | cache_miss_w);
  assign cpu_ready = cache_hit_w | ~uncached & cache_miss_w & mem_ready;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      for (int __ri0 = 0; __ri0 < 64; __ri0++) begin
        d_valid[__ri0] <= 1'b0;
      end
    end else begin
      if (cache_write) begin
        d_tags[idx] <= tag;
        d_data[idx] <= cpu_rw ? cpu_dout : mem_dout;
        d_valid[idx] <= 1'b1;
      end
    end
  end

endmodule

