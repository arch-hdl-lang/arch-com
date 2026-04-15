module axis_joiner (
  input logic clk,
  input logic rst,
  input logic [7:0] s_axis_tdata_1,
  input logic s_axis_tvalid_1,
  output logic s_axis_tready_1,
  input logic s_axis_tlast_1,
  input logic [7:0] s_axis_tdata_2,
  input logic s_axis_tvalid_2,
  output logic s_axis_tready_2,
  input logic s_axis_tlast_2,
  input logic [7:0] s_axis_tdata_3,
  input logic s_axis_tvalid_3,
  output logic s_axis_tready_3,
  input logic s_axis_tlast_3,
  output logic [7:0] m_axis_tdata,
  output logic m_axis_tvalid,
  input logic m_axis_tready,
  output logic m_axis_tlast,
  output logic [1:0] m_axis_tuser,
  output logic busy
);

  logic [1:0] ST_IDLE;
  assign ST_IDLE = 0;
  logic [1:0] ST_S1;
  assign ST_S1 = 1;
  logic [1:0] ST_S2;
  assign ST_S2 = 2;
  logic [1:0] ST_S3;
  assign ST_S3 = 3;
  logic [1:0] state;
  logic [7:0] temp_data;
  logic temp_valid;
  logic temp_last;
  logic [1:0] temp_user;
  logic temp_flag;
  logic [7:0] sel_data;
  logic sel_valid;
  logic sel_last;
  logic [1:0] sel_user;
  always_comb begin
    sel_data = 0;
    sel_valid = 1'b0;
    sel_last = 1'b0;
    sel_user = 0;
    if (state == ST_IDLE) begin
      if (s_axis_tvalid_1) begin
        sel_data = s_axis_tdata_1;
        sel_valid = s_axis_tvalid_1;
        sel_last = s_axis_tlast_1;
        sel_user = 1;
      end else if (s_axis_tvalid_2) begin
        sel_data = s_axis_tdata_2;
        sel_valid = s_axis_tvalid_2;
        sel_last = s_axis_tlast_2;
        sel_user = 2;
      end else if (s_axis_tvalid_3) begin
        sel_data = s_axis_tdata_3;
        sel_valid = s_axis_tvalid_3;
        sel_last = s_axis_tlast_3;
        sel_user = 3;
      end
    end else if (state == ST_S1) begin
      sel_data = s_axis_tdata_1;
      sel_valid = s_axis_tvalid_1;
      sel_last = s_axis_tlast_1;
      sel_user = 1;
    end else if (state == ST_S2) begin
      sel_data = s_axis_tdata_2;
      sel_valid = s_axis_tvalid_2;
      sel_last = s_axis_tlast_2;
      sel_user = 2;
    end else if (state == ST_S3) begin
      sel_data = s_axis_tdata_3;
      sel_valid = s_axis_tvalid_3;
      sel_last = s_axis_tlast_3;
      sel_user = 3;
    end
  end
  always_comb begin
    if (temp_flag) begin
      m_axis_tdata = temp_data;
      m_axis_tvalid = temp_valid;
      m_axis_tlast = temp_last;
      m_axis_tuser = temp_user;
    end else begin
      m_axis_tdata = sel_data;
      m_axis_tvalid = sel_valid;
      m_axis_tlast = sel_last;
      m_axis_tuser = sel_user;
    end
  end
  always_comb begin
    s_axis_tready_1 = 1'b0;
    s_axis_tready_2 = 1'b0;
    s_axis_tready_3 = 1'b0;
    if (state == ST_S1) begin
      s_axis_tready_1 = 1'b1;
    end else if (state == ST_S2) begin
      s_axis_tready_2 = 1'b1;
    end else if (state == ST_S3) begin
      s_axis_tready_3 = 1'b1;
    end else if (state == ST_IDLE) begin
      if (s_axis_tvalid_1) begin
        s_axis_tready_1 = 1'b1;
      end else if (s_axis_tvalid_2) begin
        s_axis_tready_2 = 1'b1;
      end else if (s_axis_tvalid_3) begin
        s_axis_tready_3 = 1'b1;
      end
    end
  end
  assign busy = state != ST_IDLE;
  always_ff @(posedge clk or posedge rst) begin
    if (rst) begin
      state <= 0;
      temp_data <= 0;
      temp_flag <= 1'b0;
      temp_last <= 1'b0;
      temp_user <= 0;
      temp_valid <= 1'b0;
    end else begin
      if (temp_flag) begin
        if (m_axis_tready) begin
          temp_flag <= 1'b0;
        end
      end else if (~m_axis_tready) begin
        if (sel_valid) begin
          temp_data <= sel_data;
          temp_valid <= sel_valid;
          temp_last <= sel_last;
          temp_user <= sel_user;
          temp_flag <= 1'b1;
        end
      end
      if (state == ST_IDLE) begin
        if (s_axis_tvalid_1) begin
          state <= ST_S1;
        end else if (s_axis_tvalid_2) begin
          state <= ST_S2;
        end else if (s_axis_tvalid_3) begin
          state <= ST_S3;
        end
      end else if (state == ST_S1) begin
        if (s_axis_tvalid_1 & &s_axis_tlast_1) begin
          state <= ST_IDLE;
        end
      end else if (state == ST_S2) begin
        if (s_axis_tvalid_2 & &s_axis_tlast_2) begin
          state <= ST_IDLE;
        end
      end else if (state == ST_S3) begin
        if (s_axis_tvalid_3 & &s_axis_tlast_3) begin
          state <= ST_IDLE;
        end
      end
    end
  end

endmodule

