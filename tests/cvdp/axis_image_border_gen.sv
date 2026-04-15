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
  logic [15:0] LAST_COL;
  assign LAST_COL = 16'($unsigned(IMG_WIDTH + 1));
  logic [15:0] LAST_ROW;
  assign LAST_ROW = 16'($unsigned(IMG_HEIGHT + 1));
  logic [15:0] x_count;
  logic [15:0] y_count;
  logic active;
  logic is_border;
  logic out_valid;
  logic at_col_end;
  logic at_row_end;
  logic m_handshake;
  logic [DATA_WIDTH-1:0] border_pixel;
  assign is_border = (x_count == 16'($unsigned(0))) | (x_count == LAST_COL) | (y_count == 16'($unsigned(0))) | (y_count == LAST_ROW);
  assign at_col_end = x_count == LAST_COL;
  assign at_row_end = at_col_end & (y_count == LAST_ROW);
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
  assign m_axis_tuser = active & (x_count == 16'($unsigned(0))) & (y_count == 16'($unsigned(0)));
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

