// AXI-Stream RGB565 to YCbCr converter with internal FIFO
module axis_rgb2ycbcr (
  input logic aclk,
  input logic aresetn,
  input logic [15:0] s_axis_tdata,
  input logic s_axis_tvalid,
  output logic s_axis_tready,
  input logic s_axis_tlast,
  input logic s_axis_tuser,
  output logic [15:0] m_axis_tdata,
  output logic m_axis_tvalid,
  input logic m_axis_tready,
  output logic m_axis_tlast,
  output logic m_axis_tuser
);

  // AXI-Stream slave (input)
  // AXI-Stream master (output)
  // FIFO depth = 16
  logic [15:0] [15:0] fifo_data;
  logic [15:0] fifo_last;
  logic [15:0] fifo_user;
  logic [3:0] write_ptr;
  logic [3:0] read_ptr;
  logic [4:0] fifo_count;
  // Status signals
  logic full;
  logic empty;
  logic fifo_read;
  logic fifo_write;
  // Extract RGB components and expand to 8-bit
  logic [7:0] r8;
  assign r8 = 8'($unsigned(s_axis_tdata[15:11])) << 3;
  logic [7:0] g8;
  assign g8 = 8'($unsigned(s_axis_tdata[10:5])) << 2;
  logic [7:0] b8;
  assign b8 = 8'($unsigned(s_axis_tdata[4:0])) << 3;
  // Widen to 16 bits unsigned for multiply, keep unsigned for Y channel
  logic [15:0] r16;
  assign r16 = 16'($unsigned(r8));
  logic [15:0] g16;
  assign g16 = 16'($unsigned(g8));
  logic [15:0] b16;
  assign b16 = 16'($unsigned(b8));
  // Y  = 16 + (77*R + 150*G + 29*B) >> 8  (all positive, unsigned is fine)
  logic [15:0] y_prod;
  assign y_prod = 16'(16'd77 * r16 + 16'd150 * g16 + 16'd29 * b16);
  logic [15:0] y_shifted;
  assign y_shifted = y_prod >> 8;
  logic [15:0] y_raw;
  assign y_raw = 16'(16'd16 + y_shifted);
  // For Cb and Cr we need signed math
  logic signed [9:0] r_s;
  assign r_s = $signed(10'($unsigned(r8)));
  logic signed [9:0] g_s;
  assign g_s = $signed(10'($unsigned(g8)));
  logic signed [9:0] b_s;
  assign b_s = $signed(10'($unsigned(b8)));
  // Cb = 128 + (-43*R - 85*G + 128*B) >> 8
  logic signed [19:0] cb_t0;
  assign cb_t0 = $signed(10'd43) * r_s;
  logic signed [19:0] cb_t1;
  assign cb_t1 = $signed(10'd85) * g_s;
  logic signed [19:0] cb_t2;
  assign cb_t2 = $signed(10'd128) * b_s;
  logic signed [21:0] cb_neg;
  assign cb_neg = 22'(({{(22-$bits(cb_t2)){cb_t2[$bits(cb_t2)-1]}}, cb_t2} - {{(22-$bits(cb_t0)){cb_t0[$bits(cb_t0)-1]}}, cb_t0}) - {{(22-$bits(cb_t1)){cb_t1[$bits(cb_t1)-1]}}, cb_t1});
  logic signed [21:0] cb_shifted;
  assign cb_shifted = cb_neg >>> 8;
  logic signed [21:0] cb_raw;
  assign cb_raw = 22'($signed(22'd128) + cb_shifted);
  // Cr = 128 + (128*R - 107*G - 21*B) >> 8
  logic signed [19:0] cr_t0;
  assign cr_t0 = $signed(10'd128) * r_s;
  logic signed [19:0] cr_t1;
  assign cr_t1 = $signed(10'd107) * g_s;
  logic signed [19:0] cr_t2;
  assign cr_t2 = $signed(10'd21) * b_s;
  logic signed [21:0] cr_neg;
  assign cr_neg = 22'(({{(22-$bits(cr_t0)){cr_t0[$bits(cr_t0)-1]}}, cr_t0} - {{(22-$bits(cr_t1)){cr_t1[$bits(cr_t1)-1]}}, cr_t1}) - {{(22-$bits(cr_t2)){cr_t2[$bits(cr_t2)-1]}}, cr_t2});
  logic signed [21:0] cr_shifted;
  assign cr_shifted = cr_neg >>> 8;
  logic signed [21:0] cr_raw;
  assign cr_raw = 22'($signed(22'd128) + cr_shifted);
  // Clamping
  logic [7:0] y_clamped;
  logic [7:0] cb_clamped;
  logic [7:0] cr_clamped;
  always_comb begin
    if (y_raw > 16'd235) begin
      y_clamped = 8'd235;
    end else if (y_raw < 16'd16) begin
      y_clamped = 8'd16;
    end else begin
      y_clamped = 8'(y_raw);
    end
    if (cb_raw > $signed(22'd240)) begin
      cb_clamped = 8'd240;
    end else if (cb_raw < $signed(22'd16)) begin
      cb_clamped = 8'd16;
    end else begin
      cb_clamped = 8'($unsigned(cb_raw));
    end
    if (cr_raw > $signed(22'd240)) begin
      cr_clamped = 8'd240;
    end else if (cr_raw < $signed(22'd16)) begin
      cr_clamped = 8'd16;
    end else begin
      cr_clamped = 8'($unsigned(cr_raw));
    end
  end
  // Pack YCbCr as 5-6-5
  logic [15:0] ycbcr_packed;
  assign ycbcr_packed = (16'($unsigned(y_clamped)) & 16'd248) << 8 | (16'($unsigned(cb_clamped)) & 16'd252) << 3 | 16'($unsigned(cr_clamped)) >> 3;
  // Handshake: accept input when FIFO not full
  assign full = fifo_count == 5'd16;
  assign empty = fifo_count == 5'd0;
  assign s_axis_tready = ~full;
  assign fifo_write = s_axis_tvalid & ~full;
  assign fifo_read = ~empty & m_axis_tready;
  // Write converted data directly to FIFO (no pipeline delay)
  always_ff @(posedge aclk) begin
    if ((!aresetn)) begin
      fifo_count <= 0;
      for (int __ri0 = 0; __ri0 < 16; __ri0++) begin
        fifo_data[__ri0] <= 0;
      end
      for (int __ri0 = 0; __ri0 < 16; __ri0++) begin
        fifo_last[__ri0] <= 1'b0;
      end
      for (int __ri0 = 0; __ri0 < 16; __ri0++) begin
        fifo_user[__ri0] <= 1'b0;
      end
      read_ptr <= 0;
      write_ptr <= 0;
    end else begin
      if (fifo_write) begin
        fifo_data[write_ptr] <= ycbcr_packed;
        fifo_last[write_ptr] <= s_axis_tlast;
        fifo_user[write_ptr] <= s_axis_tuser;
        write_ptr <= 4'(write_ptr + 4'd1);
      end
      if (fifo_read) begin
        read_ptr <= 4'(read_ptr + 4'd1);
      end
      if (fifo_write & ~fifo_read) begin
        fifo_count <= 5'(fifo_count + 5'd1);
      end else if (~fifo_write & fifo_read) begin
        fifo_count <= 5'(fifo_count - 5'd1);
      end
    end
  end
  // Output from FIFO
  assign m_axis_tdata = fifo_data[read_ptr];
  assign m_axis_tvalid = ~empty;
  assign m_axis_tlast = fifo_last[read_ptr];
  assign m_axis_tuser = fifo_user[read_ptr];

endmodule

