module pipelined_skid_buffer (
  input logic clock,
  input logic rst,
  input logic [4-1:0] i_data,
  input logic i_valid,
  input logic ready_i,
  output logic [4-1:0] data_o,
  output logic valid_o,
  output logic ready_o
);

  logic [4-1:0] s0_data;
  logic s0_valid;
  logic s0_ready;
  logic [4-1:0] r1_data;
  logic r1_valid;
  logic r1_ready;
  logic [4-1:0] s2_data;
  logic s2_valid;
  logic s2_ready;
  logic [4-1:0] r3_data;
  logic r3_valid;
  logic r3_ready;
  skid_buffer skid_0 (
    .clk(clock),
    .rst(rst),
    .i_data(i_data),
    .i_valid(i_valid),
    .i_ready(r1_ready),
    .o_data(s0_data),
    .o_valid(s0_valid),
    .o_ready(s0_ready)
  );
  register reg1 (
    .clk(clock),
    .rst(rst),
    .data_in(s0_data),
    .valid_in(s0_valid),
    .ready_in(s2_ready),
    .data_out(r1_data),
    .valid_out(r1_valid),
    .ready_out(r1_ready)
  );
  skid_buffer skid_2 (
    .clk(clock),
    .rst(rst),
    .i_data(r1_data),
    .i_valid(r1_valid),
    .i_ready(r3_ready),
    .o_data(s2_data),
    .o_valid(s2_valid),
    .o_ready(s2_ready)
  );
  register reg3 (
    .clk(clock),
    .rst(rst),
    .data_in(s2_data),
    .valid_in(s2_valid),
    .ready_in(ready_i),
    .data_out(r3_data),
    .valid_out(r3_valid),
    .ready_out(r3_ready)
  );
  assign data_o = r3_data;
  assign valid_o = r3_valid;
  assign ready_o = s0_ready;

endmodule

module register (
  input logic clk,
  input logic rst,
  input logic [4-1:0] data_in,
  input logic valid_in,
  input logic ready_in,
  output logic [4-1:0] data_out,
  output logic valid_out,
  output logic ready_out
);

  logic [4-1:0] mem;
  logic data_present;
  assign ready_out = ~data_present | ready_in;
  assign valid_out = data_present;
  assign data_out = mem;
  always_ff @(posedge clk) begin
    if (rst) begin
      data_present <= 1'b0;
      mem <= 0;
    end else begin
      if (rst) begin
        mem <= 0;
        data_present <= 1'b0;
      end else if (data_present) begin
        if (ready_in) begin
          if (valid_in) begin
            mem <= data_in;
            data_present <= 1'b1;
          end else begin
            mem <= 0;
            data_present <= 1'b0;
          end
        end
      end else if (valid_in) begin
        mem <= data_in;
        data_present <= 1'b1;
      end
    end
  end

endmodule

module skid_buffer (
  input logic clk,
  input logic rst,
  input logic [4-1:0] i_data,
  input logic i_valid,
  input logic i_ready,
  output logic [4-1:0] o_data,
  output logic o_valid,
  output logic o_ready
);

  logic [4-1:0] data_reg;
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
      if (rst) begin
        buf_flag <= 1'b0;
        data_reg <= 0;
      end else if (buf_flag) begin
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

