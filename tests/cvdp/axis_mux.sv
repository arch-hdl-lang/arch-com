// AXI Stream Multiplexer: selects one of NUM_INPUTS AXI streams based on sel
module axis_mux #(
  parameter int C_AXIS_DATA_WIDTH = 32,
  parameter int C_AXIS_TUSER_WIDTH = 4,
  parameter int C_AXIS_TID_WIDTH = 2,
  parameter int C_AXIS_TDEST_WIDTH = 2,
  parameter int NUM_INPUTS = 4
) (
  input logic aclk,
  input logic aresetn,
  input logic [2-1:0] sel,
  input logic s_axis_tvalid [NUM_INPUTS-1:0],
  output logic s_axis_tready [NUM_INPUTS-1:0],
  input logic [128-1:0] s_axis_tdata,
  input logic [16-1:0] s_axis_tkeep,
  input logic s_axis_tlast [NUM_INPUTS-1:0],
  input logic [8-1:0] s_axis_tid,
  input logic [8-1:0] s_axis_tdest,
  input logic [16-1:0] s_axis_tuser,
  output logic m_axis_tvalid,
  input logic m_axis_tready,
  output logic [C_AXIS_DATA_WIDTH-1:0] m_axis_tdata,
  output logic [4-1:0] m_axis_tkeep,
  output logic m_axis_tlast,
  output logic [C_AXIS_TID_WIDTH-1:0] m_axis_tid,
  output logic [C_AXIS_TDEST_WIDTH-1:0] m_axis_tdest,
  output logic [C_AXIS_TUSER_WIDTH-1:0] m_axis_tuser
);

  // Combinational mux: directly select input based on sel
  assign m_axis_tvalid = s_axis_tvalid[sel];
  assign m_axis_tlast = s_axis_tlast[sel];
  always_comb begin
    if (sel == 0) begin
      m_axis_tdata = s_axis_tdata[31:0];
      m_axis_tkeep = s_axis_tkeep[3:0];
      m_axis_tid = s_axis_tid[1:0];
      m_axis_tdest = s_axis_tdest[1:0];
      m_axis_tuser = s_axis_tuser[3:0];
    end else if (sel == 1) begin
      m_axis_tdata = s_axis_tdata[63:32];
      m_axis_tkeep = s_axis_tkeep[7:4];
      m_axis_tid = s_axis_tid[3:2];
      m_axis_tdest = s_axis_tdest[3:2];
      m_axis_tuser = s_axis_tuser[7:4];
    end else if (sel == 2) begin
      m_axis_tdata = s_axis_tdata[95:64];
      m_axis_tkeep = s_axis_tkeep[11:8];
      m_axis_tid = s_axis_tid[5:4];
      m_axis_tdest = s_axis_tdest[5:4];
      m_axis_tuser = s_axis_tuser[11:8];
    end else begin
      m_axis_tdata = s_axis_tdata[127:96];
      m_axis_tkeep = s_axis_tkeep[15:12];
      m_axis_tid = s_axis_tid[7:6];
      m_axis_tdest = s_axis_tdest[7:6];
      m_axis_tuser = s_axis_tuser[15:12];
    end
  end
  // Ready: route m_axis_tready back to selected input only
  always_comb begin
    if (sel == 0) begin
      s_axis_tready[0] = m_axis_tready;
      s_axis_tready[1] = 1'b0;
      s_axis_tready[2] = 1'b0;
      s_axis_tready[3] = 1'b0;
    end else if (sel == 1) begin
      s_axis_tready[0] = 1'b0;
      s_axis_tready[1] = m_axis_tready;
      s_axis_tready[2] = 1'b0;
      s_axis_tready[3] = 1'b0;
    end else if (sel == 2) begin
      s_axis_tready[0] = 1'b0;
      s_axis_tready[1] = 1'b0;
      s_axis_tready[2] = m_axis_tready;
      s_axis_tready[3] = 1'b0;
    end else begin
      s_axis_tready[0] = 1'b0;
      s_axis_tready[1] = 1'b0;
      s_axis_tready[2] = 1'b0;
      s_axis_tready[3] = m_axis_tready;
    end
  end

endmodule

