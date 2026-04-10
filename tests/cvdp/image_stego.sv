module image_stego #(
  parameter int row = 2,
  parameter int col = 2
) (
  input logic [2-1:0] bpp,
  input logic [row * col * 8-1:0] img_in,
  input logic [row * col * 4-1:0] data_in,
  output logic [row * col * 8-1:0] img_out
);

  logic [row * col * 8-1:0] out_val;
  always_comb begin
    out_val = img_in;
    for (int i = 0; i <= row * col - 1; i++) begin
      out_val[8 * i +: 8] = ((bpp == 0) ? {img_in[8 * i + 1 +: 7], data_in[i +: 1]} : ((bpp == 1) ? {img_in[8 * i + 2 +: 6], data_in[2 * i +: 2]} : ((bpp == 2) ? {img_in[8 * i + 3 +: 5], data_in[3 * i +: 3]} : {img_in[8 * i + 4 +: 4], data_in[4 * i +: 4]})));
    end
    img_out = out_val;
  end

endmodule

