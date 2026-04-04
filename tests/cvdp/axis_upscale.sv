module axis_upscale (
  input logic clk,
  input logic resetn,
  input logic dfmt_enable,
  input logic dfmt_type,
  input logic dfmt_se,
  input logic s_axis_valid,
  input logic [24-1:0] s_axis_data,
  input logic m_axis_ready,
  output logic s_axis_ready,
  output logic m_axis_valid,
  output logic [32-1:0] m_axis_data
);

  logic r_valid;
  logic [32-1:0] r_data;
  logic r_ready;
  logic [32-1:0] fmt_data;
  logic [1-1:0] bit23;
  logic [8-1:0] ext_bits;
  always_comb begin
    // Determine bit[23]: dfmt_type=1 inverts MSB, dfmt_type=0 passes through
    if (dfmt_type) begin
      bit23 = ~s_axis_data[23:23];
    end else begin
      bit23 = s_axis_data[23:23];
    end
    // Sign extension uses the NEW bit23 (after dfmt_type transform)
    if (dfmt_se) begin
      ext_bits = {bit23, bit23, bit23, bit23, bit23, bit23, bit23, bit23};
    end else begin
      ext_bits = 0;
    end
    if (dfmt_enable) begin
      fmt_data = {ext_bits, bit23, s_axis_data[22:0]};
    end else begin
      fmt_data = {8'($unsigned(0)), s_axis_data};
    end
  end
  always_ff @(posedge clk) begin
    if ((!resetn)) begin
      r_data <= 0;
      r_ready <= 1'b0;
      r_valid <= 1'b0;
    end else begin
      r_valid <= s_axis_valid;
      r_data <= fmt_data;
      r_ready <= m_axis_ready;
    end
  end
  assign m_axis_valid = r_valid;
  assign m_axis_data = r_data;
  assign s_axis_ready = r_ready;

endmodule

