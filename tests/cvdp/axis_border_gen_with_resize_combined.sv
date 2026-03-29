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

  logic [16-1:0] X_SCALE;
  assign X_SCALE = 16'($unsigned(IMG_WIDTH_IN / IMG_WIDTH_OUT));
  logic [16-1:0] Y_SCALE;
  assign Y_SCALE = 16'($unsigned(IMG_HEIGHT_IN / IMG_HEIGHT_OUT));
  logic [16-1:0] x_count_in;
  logic [16-1:0] y_count_in;
  logic [DATA_WIDTH-1:0] m_axis_tdata_r;
  logic m_axis_tvalid_r;
  logic m_axis_tlast_r;
  logic m_axis_tuser_r;
  logic valid_handshake;
  logic x_matches;
  logic y_matches;
  logic pixel_selected;
  logic is_last_col_out;
  assign valid_handshake = s_axis_tvalid & s_axis_tready;
  assign x_matches = x_count_in % X_SCALE == 0;
  assign y_matches = y_count_in % Y_SCALE == 0;
  assign pixel_selected = x_matches & y_matches;
  assign is_last_col_out = x_count_in == 16'(IMG_WIDTH_IN - X_SCALE);
  assign s_axis_tready = m_axis_tready;
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
      if (valid_handshake) begin
        if (pixel_selected) begin
          m_axis_tdata_r <= s_axis_tdata;
          m_axis_tvalid_r <= 1'b1;
          m_axis_tlast_r <= is_last_col_out;
          m_axis_tuser_r <= s_axis_tuser;
        end else begin
          m_axis_tvalid_r <= 1'b0;
          m_axis_tlast_r <= 1'b0;
          m_axis_tuser_r <= 1'b0;
        end
        if (x_count_in == 16'($unsigned(IMG_WIDTH_IN - 1))) begin
          x_count_in <= 0;
          if (y_count_in == 16'($unsigned(IMG_HEIGHT_IN - 1))) begin
            y_count_in <= 0;
          end else begin
            y_count_in <= 16'(y_count_in + 1);
          end
        end else begin
          x_count_in <= 16'(x_count_in + 1);
        end
      end else begin
        m_axis_tvalid_r <= 1'b0;
        m_axis_tlast_r <= 1'b0;
        m_axis_tuser_r <= 1'b0;
      end
    end
  end

endmodule

module axis_image_border_gen #(
  parameter int IMG_WIDTH = 336,
  parameter int IMG_HEIGHT = 256,
  parameter int BORDER_COLOR = 65535,
  parameter int DATA_MASK = 0
) (
  input logic clk,
  input logic resetn,
  input logic [16-1:0] s_axis_tdata,
  input logic s_axis_tvalid,
  output logic s_axis_tready,
  input logic s_axis_tlast,
  input logic s_axis_tuser,
  output logic [16-1:0] m_axis_tdata,
  output logic m_axis_tvalid,
  input logic m_axis_tready,
  output logic m_axis_tlast,
  output logic m_axis_tuser
);

  logic [16-1:0] x_count;
  logic [16-1:0] y_count;
  logic border_valid_r;
  logic frame_active;
  logic [16-1:0] border_color_16;
  assign border_color_16 = BORDER_COLOR[15:0];
  logic is_top_row;
  logic is_bottom_row;
  logic is_left_border;
  logic is_right_border;
  logic is_border_pixel;
  logic handshake;
  assign is_top_row = y_count == 0;
  assign is_bottom_row = y_count == 16'($unsigned(IMG_HEIGHT + 1));
  assign is_left_border = x_count == 0;
  assign is_right_border = x_count == 16'($unsigned(IMG_WIDTH + 1));
  assign is_border_pixel = is_top_row | is_bottom_row | is_left_border | is_right_border;
  assign handshake = m_axis_tvalid & m_axis_tready;
  assign s_axis_tready = ~is_border_pixel & m_axis_tready & frame_active;
  assign m_axis_tvalid = is_border_pixel ? border_valid_r : s_axis_tvalid & frame_active;
  assign m_axis_tdata = is_border_pixel ? border_color_16 : s_axis_tdata;
  assign m_axis_tlast = x_count == 16'($unsigned(IMG_WIDTH + 1));
  assign m_axis_tuser = s_axis_tuser;
  always_ff @(posedge clk or negedge resetn) begin
    if ((!resetn)) begin
      border_valid_r <= 1'b0;
      frame_active <= 1'b0;
      x_count <= 0;
      y_count <= 0;
    end else begin
      if (s_axis_tuser & ~frame_active) begin
        frame_active <= 1'b1;
        border_valid_r <= 1'b1;
      end
      if (handshake) begin
        if (x_count == 16'($unsigned(IMG_WIDTH + 1))) begin
          x_count <= 0;
          if (y_count == 16'($unsigned(IMG_HEIGHT + 1))) begin
            y_count <= 0;
            frame_active <= 1'b0;
            border_valid_r <= 1'b0;
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

  logic [DATA_WIDTH-1:0] resizer_tdata;
  logic resizer_tvalid;
  logic resizer_tready;
  logic resizer_tlast;
  logic resizer_tuser;
  axis_image_resizer #(.IMG_WIDTH_IN(IMG_WIDTH_IN), .IMG_HEIGHT_IN(IMG_HEIGHT_IN), .IMG_WIDTH_OUT(IMG_WIDTH_OUT), .IMG_HEIGHT_OUT(IMG_HEIGHT_OUT), .DATA_WIDTH(DATA_WIDTH)) resizer_inst (
    .clk(clk),
    .resetn(resetn),
    .s_axis_tdata(s_axis_tdata),
    .s_axis_tvalid(s_axis_tvalid),
    .s_axis_tready(s_axis_tready),
    .s_axis_tlast(s_axis_tlast),
    .s_axis_tuser(s_axis_tuser),
    .m_axis_tdata(resizer_tdata),
    .m_axis_tvalid(resizer_tvalid),
    .m_axis_tready(resizer_tready),
    .m_axis_tlast(resizer_tlast),
    .m_axis_tuser(resizer_tuser)
  );
  axis_image_border_gen #(.IMG_WIDTH(IMG_WIDTH_OUT), .IMG_HEIGHT(IMG_HEIGHT_OUT), .BORDER_COLOR(BORDER_COLOR), .DATA_MASK(0)) border_gen_inst (
    .clk(clk),
    .resetn(resetn),
    .s_axis_tdata(resizer_tdata),
    .s_axis_tvalid(resizer_tvalid),
    .s_axis_tready(resizer_tready),
    .s_axis_tlast(resizer_tlast),
    .s_axis_tuser(resizer_tuser),
    .m_axis_tdata(m_axis_tdata),
    .m_axis_tvalid(m_axis_tvalid),
    .m_axis_tready(m_axis_tready),
    .m_axis_tlast(m_axis_tlast),
    .m_axis_tuser(m_axis_tuser)
  );

endmodule

