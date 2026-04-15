// Alpha blending module: blends foreground pixels with background using alpha
// pixel_in and bg_pixel_in are H*W pixels, each 24-bit RGB packed LSB-first
// alpha_in is H*W alpha values, each 8-bit packed LSB-first
// blended_out is H*W blended pixels, each 24-bit RGB packed LSB-first
// Formula: blended_channel = (alpha*fg + (255-alpha)*bg) / 255
module alphablending #(
  parameter int H = 4,
  parameter int W = 4,
  parameter int N = 1,
  parameter int NUM_PIXELS = H * W
) (
  input logic clk,
  input logic reset,
  input logic start,
  input logic [H * W * 24-1:0] pixel_in,
  input logic [H * W * 24-1:0] bg_pixel_in,
  input logic [H * W * 8-1:0] alpha_in,
  output logic [H * W * 24-1:0] blended_out,
  output logic done
);

  logic running_r;
  logic finished_r;
  logic [15:0] pix_idx_r;
  // Blended output accumulator: NUM_PIXELS * 24 bits
  logic [H * W * 24-1:0] blend_r;
  // Extract current pixel's data
  logic [23:0] fg_pix;
  logic [23:0] bg_pix;
  logic [7:0] alp_pix;
  assign fg_pix = pixel_in[pix_idx_r * 24 +: 24];
  assign bg_pix = bg_pixel_in[pix_idx_r * 24 +: 24];
  assign alp_pix = alpha_in[pix_idx_r * 8 +: 8];
  // Extract 24-bit foreground, background pixel and 8-bit alpha for current index
  // Per-channel blending: blended = (alpha*fg + (255-alpha)*bg) / 255
  logic [8:0] alp_ext;
  logic [8:0] alp_inv;
  assign alp_ext = 9'($unsigned(alp_pix));
  assign alp_inv = 9'(255 - alp_ext);
  logic [7:0] fg_r;
  logic [7:0] fg_g;
  logic [7:0] fg_b;
  logic [7:0] bg_r;
  logic [7:0] bg_g;
  logic [7:0] bg_b;
  assign fg_r = fg_pix[23:16];
  assign fg_g = fg_pix[15:8];
  assign fg_b = fg_pix[7:0];
  assign bg_r = bg_pix[23:16];
  assign bg_g = bg_pix[15:8];
  assign bg_b = bg_pix[7:0];
  // alp_ext max = 255 (9-bit), fg_r max = 255 (8-bit) → product max = 65025 (17-bit)
  // sum of two products max = 130050 (18-bit)
  logic [17:0] blend_r_ch;
  logic [17:0] blend_g_ch;
  logic [17:0] blend_b_ch;
  assign blend_r_ch = 18'(17'($unsigned(alp_ext)) * 17'($unsigned(fg_r)) + 17'($unsigned(alp_inv)) * 17'($unsigned(bg_r)));
  assign blend_g_ch = 18'(17'($unsigned(alp_ext)) * 17'($unsigned(fg_g)) + 17'($unsigned(alp_inv)) * 17'($unsigned(bg_g)));
  assign blend_b_ch = 18'(17'($unsigned(alp_ext)) * 17'($unsigned(fg_b)) + 17'($unsigned(alp_inv)) * 17'($unsigned(bg_b)));
  // Divide by 255 to get 8-bit result
  logic [7:0] out_r;
  logic [7:0] out_g;
  logic [7:0] out_b;
  assign out_r = 8'(blend_r_ch / 255);
  assign out_g = 8'(blend_g_ch / 255);
  assign out_b = 8'(blend_b_ch / 255);
  logic [23:0] blended_pixel;
  assign blended_pixel = {out_r, out_g, out_b};
  assign done = finished_r;
  assign blended_out = blend_r;
  always_ff @(posedge clk) begin
    if (reset) begin
      running_r <= 1'b0;
      finished_r <= 1'b0;
      pix_idx_r <= 0;
      blend_r <= 0;
    end else if (start & ~running_r & ~finished_r) begin
      running_r <= 1'b1;
      finished_r <= 1'b0;
      pix_idx_r <= 0;
      blend_r <= 0;
    end else if (running_r) begin
      blend_r[pix_idx_r * 24 +: 24] <= blended_pixel;
      if (pix_idx_r == 16'($unsigned(NUM_PIXELS - 1))) begin
        running_r <= 1'b0;
        finished_r <= 1'b1;
        pix_idx_r <= 0;
      end else begin
        pix_idx_r <= 16'(pix_idx_r + 1);
      end
    end else if (finished_r & ~start) begin
      finished_r <= 1'b0;
    end
  end

endmodule

