module image_stego #(
  parameter int row = 2,
  parameter int col = 2
) (
  input logic [2-1:0] bpp,
  input logic [row * col * 8-1:0] img_in,
  input logic [row * col * 4-1:0] data_in,
  output logic [row * col * 8-1:0] img_out
);

  // Purely combinational: embed data_in into LSBs of each pixel's blue channel
  // based on bpp (bits per pixel to embed: 1, 2, 3, or 4)
  // Each pixel is 8 bits. row*col pixels total.
  // For each pixel i, embed data_in[bpp*(i+1)-1 : bpp*i] into img_in[8*i+bpp-1 : 8*i]
  logic [row * col * 8-1:0] out_val;
  always_comb begin
    out_val = img_in;
    for (int i = 0; i <= row * col - 1; i++) begin
      out_val[8 * i +: 4] = ((bpp == 0) ? {img_in[8 * i + 1 +: 3], data_in[4 * i +: 1]} : ((bpp == 1) ? {img_in[8 * i + 2 +: 2], data_in[4 * i +: 2]} : ((bpp == 2) ? {img_in[8 * i + 3 +: 1], data_in[4 * i +: 3]} : data_in[4 * i +: 4])));
    end
    img_out = out_val;
  end

endmodule

