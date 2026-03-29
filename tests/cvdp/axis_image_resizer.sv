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

