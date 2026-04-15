module data_width_converter (
  input logic clk,
  input logic reset,
  input logic [31:0] data_in,
  input logic data_valid,
  output logic [127:0] o_data_out = 0,
  output logic o_data_out_valid = 0
);

  logic [1:0] cnt = 0;
  logic [31:0] buf0 = 0;
  logic [31:0] buf1 = 0;
  logic [31:0] buf2 = 0;
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      buf0 <= 0;
      buf1 <= 0;
      buf2 <= 0;
      cnt <= 0;
      o_data_out <= 0;
      o_data_out_valid <= 0;
    end else begin
      o_data_out_valid <= 1'b0;
      if (data_valid & (cnt == 0)) begin
        buf0 <= data_in;
      end else if (data_valid & (cnt == 1)) begin
        buf1 <= data_in;
      end else if (data_valid & (cnt == 2)) begin
        buf2 <= data_in;
      end else if (data_valid & (cnt == 3)) begin
        o_data_out <= {buf0, buf1, buf2, data_in};
      end
      if (data_valid & (cnt == 3)) begin
        o_data_out_valid <= 1'b1;
      end
      if (data_valid & (cnt == 3)) begin
        cnt <= 0;
      end else if (data_valid) begin
        cnt <= 2'(cnt + 1);
      end
    end
  end

endmodule

