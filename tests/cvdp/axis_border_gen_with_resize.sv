module axis_border_gen_with_resize #(
  parameter int IMG_WIDTH_IN = 640,
  parameter int IMG_HEIGHT_IN = 480,
  parameter int IMG_WIDTH_OUT = 320,
  parameter int IMG_HEIGHT_OUT = 240,
  parameter int BORDER_COLOR = 65535,
  parameter int DATA_WIDTH = 16
) (
  input logic clk,
  input logic resetn,
  input logic [DATA_WIDTH-1:0] s_axis_tdata,
  input logic s_axis_tvalid,
  output logic s_axis_tready,
  input logic s_axis_tlast,
  input logic s_axis_tuser,
  output logic [DATA_WIDTH-1:0] m_axis_tdata,
  output logic m_axis_tvalid,
  input logic m_axis_tready,
  output logic m_axis_tlast,
  output logic m_axis_tuser
);

  // Internal wires between resizer and border gen
  logic [DATA_WIDTH-1:0] mid_tdata;
  logic mid_tvalid;
  logic mid_tready;
  logic mid_tlast;
  logic mid_tuser;
  axis_image_resizer #(.IMG_WIDTH_IN(IMG_WIDTH_IN), .IMG_HEIGHT_IN(IMG_HEIGHT_IN), .IMG_WIDTH_OUT(IMG_WIDTH_OUT), .IMG_HEIGHT_OUT(IMG_HEIGHT_OUT), .DATA_WIDTH(DATA_WIDTH)) resizer (
    .clk(clk),
    .resetn(resetn),
    .s_axis_tdata(s_axis_tdata),
    .s_axis_tvalid(s_axis_tvalid),
    .s_axis_tready(s_axis_tready),
    .s_axis_tlast(s_axis_tlast),
    .s_axis_tuser(s_axis_tuser),
    .m_axis_tdata(mid_tdata),
    .m_axis_tvalid(mid_tvalid),
    .m_axis_tready(mid_tready),
    .m_axis_tlast(mid_tlast),
    .m_axis_tuser(mid_tuser)
  );
  axis_image_border_gen #(.IMG_WIDTH(IMG_WIDTH_OUT), .IMG_HEIGHT(IMG_HEIGHT_OUT), .BORDER_COLOR(BORDER_COLOR), .DATA_WIDTH(DATA_WIDTH)) border_gen (
    .clk(clk),
    .resetn(resetn),
    .s_axis_tdata(mid_tdata),
    .s_axis_tvalid(mid_tvalid),
    .s_axis_tready(mid_tready),
    .s_axis_tlast(mid_tlast),
    .s_axis_tuser(mid_tuser),
    .m_axis_tdata(m_axis_tdata),
    .m_axis_tvalid(m_axis_tvalid),
    .m_axis_tready(m_axis_tready),
    .m_axis_tlast(m_axis_tlast),
    .m_axis_tuser(m_axis_tuser)
  );

endmodule

module axis_image_border_gen #(
  parameter int IMG_WIDTH = 336,
  parameter int IMG_HEIGHT = 256,
  parameter int BORDER_COLOR = 65535,
  parameter int DATA_WIDTH = 16
) (
  input logic clk,
  input logic resetn,
  input logic [DATA_WIDTH-1:0] s_axis_tdata,
  input logic s_axis_tvalid,
  output logic s_axis_tready,
  input logic s_axis_tlast,
  input logic s_axis_tuser,
  output logic [DATA_WIDTH-1:0] m_axis_tdata,
  output logic m_axis_tvalid,
  input logic m_axis_tready,
  output logic m_axis_tlast,
  output logic m_axis_tuser
);

  // Output image is (IMG_WIDTH+2) wide, (IMG_HEIGHT+2) tall
  logic [16-1:0] LAST_COL;
  assign LAST_COL = 16'($unsigned(IMG_WIDTH + 1));
  logic [16-1:0] LAST_ROW;
  assign LAST_ROW = 16'($unsigned(IMG_HEIGHT + 1));
  logic [16-1:0] x_count;
  logic [16-1:0] y_count;
  logic active;
  logic is_border;
  logic out_valid;
  logic at_col_end;
  logic at_row_end;
  logic m_handshake;
  logic [DATA_WIDTH-1:0] border_pixel;
  assign is_border = x_count == 16'($unsigned(0)) | x_count == LAST_COL | y_count == 16'($unsigned(0)) | y_count == LAST_ROW;
  assign at_col_end = x_count == LAST_COL;
  assign at_row_end = at_col_end & y_count == LAST_ROW;
  assign border_pixel = BORDER_COLOR[DATA_WIDTH - 1:0];
  assign out_valid = active & (is_border | s_axis_tvalid);
  assign m_handshake = out_valid & m_axis_tready;
  assign s_axis_tready = active & ~is_border & m_axis_tready;
  // For border pixels: generated internally; for content: need upstream valid
  // Only accept upstream when active AND at content position AND downstream ready
  // When not active: s_axis_tready=0 so resizer holds its output for us to observe
  assign m_axis_tdata = is_border ? border_pixel : s_axis_tdata;
  assign m_axis_tvalid = out_valid;
  assign m_axis_tlast = active & at_col_end;
  assign m_axis_tuser = active & x_count == 16'($unsigned(0)) & y_count == 16'($unsigned(0));
  always_ff @(posedge clk or negedge resetn) begin
    if ((!resetn)) begin
      active <= 1'b0;
      x_count <= 0;
      y_count <= 0;
    end else begin
      if (~active) begin
        // Observe (but do not consume) upstream: activate when tuser seen
        if (s_axis_tuser) begin
          active <= 1'b1;
          x_count <= 16'($unsigned(0));
          y_count <= 16'($unsigned(0));
        end
      end else if (m_handshake) begin
        // Advance counters on every output handshake
        if (at_col_end) begin
          x_count <= 16'($unsigned(0));
          if (at_row_end) begin
            active <= 1'b0;
            y_count <= 16'($unsigned(0));
          end else begin
            y_count <= 16'(y_count + 1);
          end
        end else begin
          x_count <= 16'(x_count + 1);
        end
      end
    end
  end

endmodule

module axis_image_resizer #(
  parameter int IMG_WIDTH_IN = 640,
  parameter int IMG_HEIGHT_IN = 480,
  parameter int IMG_WIDTH_OUT = 320,
  parameter int IMG_HEIGHT_OUT = 240,
  parameter int DATA_WIDTH = 16
) (
  input logic clk,
  input logic resetn,
  input logic [DATA_WIDTH-1:0] s_axis_tdata,
  input logic s_axis_tvalid,
  output logic s_axis_tready,
  input logic s_axis_tlast,
  input logic s_axis_tuser,
  output logic [DATA_WIDTH-1:0] m_axis_tdata,
  output logic m_axis_tvalid,
  input logic m_axis_tready,
  output logic m_axis_tlast,
  output logic m_axis_tuser
);

  // Downsampling factors (compile-time constants)
  logic [16-1:0] X_SCALE;
  assign X_SCALE = 16'($unsigned(IMG_WIDTH_IN / IMG_WIDTH_OUT));
  logic [16-1:0] Y_SCALE;
  assign Y_SCALE = 16'($unsigned(IMG_HEIGHT_IN / IMG_HEIGHT_OUT));
  logic [16-1:0] WIDTH_IN;
  assign WIDTH_IN = 16'($unsigned(IMG_WIDTH_IN));
  logic [16-1:0] HEIGHT_IN;
  assign HEIGHT_IN = 16'($unsigned(IMG_HEIGHT_IN));
  // Input pixel counters
  logic [16-1:0] x_count_in;
  logic [16-1:0] y_count_in;
  // Output buffer
  logic [DATA_WIDTH-1:0] m_axis_tdata_r;
  logic m_axis_tvalid_r;
  logic m_axis_tlast_r;
  logic m_axis_tuser_r;
  logic pixel_sel;
  logic handshake;
  logic out_free;
  logic at_row_end;
  assign out_free = ~m_axis_tvalid_r | m_axis_tready;
  assign handshake = s_axis_tvalid & s_axis_tready;
  assign at_row_end = x_count_in == WIDTH_IN - 1;
  assign pixel_sel = x_count_in % X_SCALE == 16'($unsigned(0)) & y_count_in % Y_SCALE == 16'($unsigned(0));
  assign s_axis_tready = out_free;
  // Output handshake: downstream consuming current output
  // Accept input when output buffer is free
  // Ready when output buffer not holding data (or being consumed this cycle)
  // Output driven from registers
  assign m_axis_tdata = m_axis_tdata_r;
  assign m_axis_tvalid = m_axis_tvalid_r;
  assign m_axis_tlast = m_axis_tlast_r;
  assign m_axis_tuser = m_axis_tuser_r;
  always_ff @(posedge clk or negedge resetn) begin
    if ((!resetn)) begin
      m_axis_tdata_r <= 0;
      m_axis_tlast_r <= 1'b0;
      m_axis_tuser_r <= 1'b0;
      m_axis_tvalid_r <= 1'b0;
      x_count_in <= 0;
      y_count_in <= 0;
    end else begin
      // Clear valid when downstream consumes
      if (m_axis_tvalid_r & m_axis_tready) begin
        m_axis_tvalid_r <= 1'b0;
        m_axis_tlast_r <= 1'b0;
        m_axis_tuser_r <= 1'b0;
      end
      if (handshake) begin
        // Update input counters
        if (at_row_end) begin
          x_count_in <= 16'($unsigned(0));
          if (y_count_in == HEIGHT_IN - 1) begin
            y_count_in <= 16'($unsigned(0));
          end else begin
            y_count_in <= 16'(y_count_in + 1);
          end
        end else begin
          x_count_in <= 16'(x_count_in + 1);
        end
        // Emit pixel if it falls on sampling grid
        if (pixel_sel) begin
          m_axis_tdata_r <= s_axis_tdata;
          m_axis_tvalid_r <= 1'b1;
          m_axis_tlast_r <= at_row_end;
          m_axis_tuser_r <= s_axis_tuser;
        end
      end
    end
  end

endmodule

