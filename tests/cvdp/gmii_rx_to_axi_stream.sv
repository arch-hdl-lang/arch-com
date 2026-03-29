module gmii_rx_to_axi_stream (
  input logic gmii_rx_clk,
  input logic [8-1:0] gmii_rxd,
  input logic gmii_rx_dv,
  output logic [8-1:0] m_axis_tdata,
  output logic m_axis_tvalid,
  input logic m_axis_tready,
  output logic m_axis_tlast
);

  logic [2-1:0] state_r = 0;
  logic [8-1:0] data_r = 0;
  logic valid_r = 1'b0;
  logic last_r = 1'b0;
  logic dv_prev = 1'b0;
  always_ff @(posedge gmii_rx_clk) begin
    dv_prev <= gmii_rx_dv;
  end
  always_ff @(posedge gmii_rx_clk) begin
    if (state_r == 0) begin
      // IDLE
      if (gmii_rx_dv) begin
        state_r <= 1;
        data_r <= gmii_rxd;
        valid_r <= 1'b1;
        last_r <= 1'b0;
      end else begin
        valid_r <= 1'b0;
        last_r <= 1'b0;
      end
    end else if (state_r == 1) begin
      // RECEIVING
      if (gmii_rx_dv) begin
        if (m_axis_tready) begin
          data_r <= gmii_rxd;
          valid_r <= 1'b1;
        end else begin
          valid_r <= 1'b0;
        end
        last_r <= 1'b0;
      end else begin
        // dv dropped, end of frame
        state_r <= 2;
        valid_r <= 1'b1;
        last_r <= 1'b1;
      end
    end else if (m_axis_tready) begin
      // FINISHED
      state_r <= 0;
      valid_r <= 1'b0;
      last_r <= 1'b0;
    end
  end
  assign m_axis_tdata = data_r;
  assign m_axis_tvalid = valid_r;
  assign m_axis_tlast = last_r;

endmodule

