module _gmii_rx_to_axi_stream_threads (
  input logic gmii_rx_clk,
  input logic reset,
  input logic gmii_rx_dv,
  input logic [7:0] gmii_rxd,
  input logic m_axis_tready,
  output logic [7:0] m_axis_tdata,
  output logic m_axis_tlast,
  output logic m_axis_tvalid
);

  logic [2:0] _t0_state = 0;
  always_ff @(posedge gmii_rx_clk) begin
    if (reset) begin
      _t0_state <= 0;
      m_axis_tdata <= 0;
      m_axis_tlast <= 1'b0;
      m_axis_tvalid <= 1'b0;
    end else begin
      if (_t0_state == 0) begin
        // `thread` requires a reset clause. The CVDP TB doesn't drive a
        // reset signal, but `dut_init` zeroes every input — so a sync-high
        // reset port stays deasserted throughout the test.
        // `port reg` so the thread can drive these directly via `<=`.
        // 1. Idle until rx_dv asserts.
        // 2. Forward each byte while rx_dv stays high (de-asserting valid on
        //    backpressured cycles, matching the original FSM's behavior).
        // 3. When rx_dv drops, present the last marker until tready accepts.
        m_axis_tvalid <= 1'b0;
        m_axis_tlast <= 1'b0;
        _t0_state <= 1;
      end
      if (_t0_state == 1) begin
        if (gmii_rx_dv) begin
          _t0_state <= 2;
        end
      end
      if (_t0_state == 2) begin
        m_axis_tdata <= gmii_rxd;
        m_axis_tvalid <= 1'b1;
        m_axis_tlast <= 1'b0;
        _t0_state <= 3;
      end
      if (_t0_state == 3) begin
        if (m_axis_tready) begin
          m_axis_tdata <= gmii_rxd;
          m_axis_tvalid <= 1'b1;
        end else begin
          m_axis_tvalid <= 1'b0;
        end
        m_axis_tlast <= 1'b0;
        if (!gmii_rx_dv) begin
          _t0_state <= 4;
        end
      end
      if (_t0_state == 4) begin
        m_axis_tvalid <= 1'b1;
        m_axis_tlast <= 1'b1;
        _t0_state <= 5;
      end
      if (_t0_state == 5) begin
        if (m_axis_tready) begin
          _t0_state <= 0;
        end
      end
    end
  end

endmodule

module gmii_rx_to_axi_stream (
  input logic gmii_rx_clk,
  input logic reset,
  input logic [7:0] gmii_rxd,
  input logic gmii_rx_dv,
  output logic [7:0] m_axis_tdata,
  output logic m_axis_tvalid,
  output logic m_axis_tlast,
  input logic m_axis_tready
);

  _gmii_rx_to_axi_stream_threads _threads (
    .gmii_rx_clk(gmii_rx_clk),
    .reset(reset),
    .gmii_rx_dv(gmii_rx_dv),
    .gmii_rxd(gmii_rxd),
    .m_axis_tready(m_axis_tready),
    .m_axis_tdata(m_axis_tdata),
    .m_axis_tlast(m_axis_tlast),
    .m_axis_tvalid(m_axis_tvalid)
  );

endmodule

