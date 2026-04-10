// Find maximum value and its index among NumSrc valid inputs.
// Combinational priority scan with shift-register pipeline to match
// the expected latency of $clog2(NumSrc)+1 cycles.
module prim_max_find #(
  parameter int NumSrc = 8,
  parameter int Width = 8,
  localparam int SrcWidth = $clog2(NumSrc),
  localparam int LATENCY = $clog2(NumSrc) + 1
) (
  input logic clk_i,
  input logic rst_ni,
  input logic [Width * NumSrc-1:0] values_i,
  input logic [NumSrc-1:0] valid_i,
  output logic [Width-1:0] max_value_o,
  output logic [SrcWidth-1:0] max_idx_o,
  output logic max_valid_o
);

  // Combinational max-find: priority scan from index 0
  logic [Width-1:0] comb_max_val;
  logic [SrcWidth-1:0] comb_max_idx;
  logic comb_max_vld;
  always_comb begin
    comb_max_val = 0;
    comb_max_idx = 0;
    comb_max_vld = 1'b0;
    for (int i = 0; i <= NumSrc - 1; i++) begin
      if (valid_i[i +: 1]) begin
        if (comb_max_vld == 1'b0 | Width'(values_i >> i * Width) > comb_max_val) begin
          comb_max_val = Width'(values_i >> i * Width);
          comb_max_idx = SrcWidth'(i);
          comb_max_vld = 1'b1;
        end
      end
    end
  end
  // Shift-register pipeline for latency matching
  logic [Width-1:0] sr_val [LATENCY-1:0];
  logic [SrcWidth-1:0] sr_idx [LATENCY-1:0];
  logic sr_vld [LATENCY-1:0];
  always_ff @(posedge clk_i or negedge rst_ni) begin
    if ((!rst_ni)) begin
      for (int __ri0 = 0; __ri0 < LATENCY; __ri0++) begin
        sr_idx[__ri0] <= 0;
      end
      for (int __ri0 = 0; __ri0 < LATENCY; __ri0++) begin
        sr_val[__ri0] <= 0;
      end
      for (int __ri0 = 0; __ri0 < LATENCY; __ri0++) begin
        sr_vld[__ri0] <= 0;
      end
    end else begin
      sr_val[0] <= comb_max_val;
      sr_idx[0] <= comb_max_idx;
      sr_vld[0] <= comb_max_vld;
      for (int i = 1; i <= LATENCY - 1; i++) begin
        sr_val[i] <= sr_val[i - 1];
        sr_idx[i] <= sr_idx[i - 1];
        sr_vld[i] <= sr_vld[i - 1];
      end
    end
  end
  assign max_value_o = sr_val[LATENCY - 1];
  assign max_idx_o = sr_idx[LATENCY - 1];
  assign max_valid_o = sr_vld[LATENCY - 1];

endmodule

