module sync_pos_neg_edge_detector (
  input logic i_clk,
  input logic i_rstb,
  input logic i_detection_signal,
  output logic o_positive_edge_detected,
  output logic o_negative_edge_detected
);

  logic prev;
  always_ff @(posedge i_clk or negedge i_rstb) begin
    if ((!i_rstb)) begin
      o_negative_edge_detected <= 0;
      o_positive_edge_detected <= 0;
      prev <= 0;
    end else begin
      prev <= i_detection_signal;
      o_positive_edge_detected <= ~prev & i_detection_signal;
      o_negative_edge_detected <= prev & ~i_detection_signal;
    end
  end

endmodule

