module sobel_filter #(
  parameter int THRESHOLD = 128
) (
  input logic clk,
  input logic rst_n,
  input logic [8-1:0] pixel_in,
  input logic valid_in,
  output logic [8-1:0] edge_out,
  output logic valid_out
);

  // 9-pixel shift register (p0=newest, p8=oldest)
  logic [8-1:0] p0;
  logic [8-1:0] p1;
  logic [8-1:0] p2;
  logic [8-1:0] p3;
  logic [8-1:0] p4;
  logic [8-1:0] p5;
  logic [8-1:0] p6;
  logic [8-1:0] p7;
  logic [8-1:0] p8;
  // Gradient registers (computed from current buffer, used next cycle for output)
  logic signed [11-1:0] gx_r;
  logic signed [11-1:0] gy_r;
  logic [8-1:0] edge_out_r;
  logic valid_out_r;
  // Count valid_in pulses; only assert valid_out after 9 pixels loaded
  logic [4-1:0] pixel_cnt;
  logic [11-1:0] abs_gx;
  logic [11-1:0] abs_gy;
  logic [13-1:0] magnitude;
  assign abs_gx = gx_r[10] ? 11'($unsigned(~gx_r + 1)) : 11'($unsigned(gx_r));
  assign abs_gy = gy_r[10] ? 11'($unsigned(~gy_r + 1)) : 11'($unsigned(gy_r));
  assign magnitude = 13'(13'($unsigned(abs_gx)) + 13'($unsigned(abs_gy)));
  assign edge_out = edge_out_r;
  assign valid_out = valid_out_r;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      edge_out_r <= 0;
      gx_r <= 0;
      gy_r <= 0;
      p0 <= 0;
      p1 <= 0;
      p2 <= 0;
      p3 <= 0;
      p4 <= 0;
      p5 <= 0;
      p6 <= 0;
      p7 <= 0;
      p8 <= 0;
      pixel_cnt <= 0;
      valid_out_r <= 1'b0;
    end else begin
      valid_out_r <= 1'b0;
      if (valid_in) begin
        // Shift buffer
        p8 <= p7;
        p7 <= p6;
        p6 <= p5;
        p5 <= p4;
        p4 <= p3;
        p3 <= p2;
        p2 <= p1;
        p1 <= p0;
        p0 <= pixel_in;
        // Sobel Gx: img[0][0] - img[0][2] + 2*img[1][0] - 2*img[1][2] + img[2][0] - img[2][2]
        // With p8=img[0][0]...p0=img[2][2]:
        gx_r <= 11'($signed(p8) - $signed(p6) + ($signed(p5) << 1) - ($signed(p3) << 1) + $signed(p2) - $signed(p0));
        // Sobel Gy: img[0][0] + 2*img[0][1] + img[0][2] - img[2][0] - 2*img[2][1] - img[2][2]
        gy_r <= 11'($signed(p8) + ($signed(p7) << 1) + $signed(p6) - $signed(p2) - ($signed(p1) << 1) - $signed(p0));
        // Threshold from PREVIOUS gx_r/gy_r (pipeline delay)
        edge_out_r <= magnitude > 13'($unsigned(THRESHOLD)) ? 8'd255 : 8'd0;
        // Count pixels; valid_out asserts after 9th pixel (and on every subsequent pixel)
        if (pixel_cnt < 9) begin
          pixel_cnt <= 4'(pixel_cnt + 1);
        end
        if (pixel_cnt >= 9) begin
          valid_out_r <= 1'b1;
        end
      end else begin
        // Reset counter when valid_in deasserts
        pixel_cnt <= 0;
      end
    end
  end

endmodule

