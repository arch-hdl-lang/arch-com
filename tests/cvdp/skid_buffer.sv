module skid_buffer (
  input logic clk,
  input logic rst,
  input logic [3:0] i_data,
  input logic i_valid,
  input logic i_ready,
  output logic [3:0] o_data,
  output logic o_valid,
  output logic o_ready
);

  logic [3:0] data_reg;
  logic buf_flag;
  // o_ready: we can accept data when buffer is empty
  assign o_ready = ~buf_flag;
  // o_valid: valid when input valid or buffer has data
  assign o_valid = i_valid | buf_flag;
  // o_data: output buffered data if buffer active, else pass through
  always_comb begin
    if (buf_flag) begin
      o_data = data_reg;
    end else begin
      o_data = i_data;
    end
  end
  always_ff @(posedge clk) begin
    if (rst) begin
      buf_flag <= 1'b0;
      data_reg <= 0;
    end else begin
      if (buf_flag) begin
        // buffer has data, downstream ready => release
        if (i_ready) begin
          buf_flag <= 1'b0;
          data_reg <= 0;
        end
      end else if (i_valid & ~i_ready) begin
        // no buffer: if input valid and downstream not ready, store
        buf_flag <= 1'b1;
        data_reg <= i_data;
      end
    end
  end

endmodule

