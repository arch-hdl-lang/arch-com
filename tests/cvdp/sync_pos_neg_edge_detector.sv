module sync_pos_neg_edge_detector (
  input logic i_clk,
  input logic i_rstb,
  input logic i_detection_signal,
  output logic o_positive_edge_detected,
  output logic o_negative_edge_detected
);

  logic sig_d1;
  logic sig_d2;
  always_ff @(posedge i_clk or negedge i_rstb) begin
    if ((!i_rstb)) begin
      sig_d1 <= 0;
      sig_d2 <= 0;
    end else begin
      sig_d1 <= i_detection_signal;
      sig_d2 <= sig_d1;
    end
  end
  assign o_positive_edge_detected = sig_d1 & ~sig_d2;
  assign o_negative_edge_detected = ~sig_d1 & sig_d2;

endmodule

