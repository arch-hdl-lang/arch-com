// AXI stream downsizer: 16-bit input to 8-bit output (2 beats per transfer)
// First beat: high byte [15:8], second beat: low byte [7:0]
module axis_resize (
  input logic clk,
  input logic resetn,
  input logic s_valid,
  output logic s_ready,
  input logic [15:0] s_data,
  output logic m_valid,
  input logic m_ready,
  output logic [7:0] m_data
);

  // phase 0: idle/accept, phase 1: sending low byte
  logic [0:0] phase;
  logic [7:0] data_buf;
  logic m_valid_r;
  logic [7:0] m_data_r;
  always_ff @(posedge clk) begin
    if ((!resetn)) begin
      data_buf <= 0;
      m_data_r <= 0;
      m_valid_r <= 1'b0;
      phase <= 0;
    end else begin
      if (phase == 0) begin
        if (s_valid & s_ready) begin
          // Latch high byte out, save low byte
          m_data_r <= s_data[15:8];
          data_buf <= s_data[7:0];
          m_valid_r <= 1'b1;
          phase <= 1;
        end
      end else if (m_ready) begin
        // Send low byte
        m_data_r <= data_buf;
        m_valid_r <= 1'b1;
        phase <= 0;
      end
    end
  end
  assign s_ready = ~phase;
  assign m_valid = m_valid_r;
  assign m_data = m_data_r;

endmodule

