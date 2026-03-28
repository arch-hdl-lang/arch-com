module sync_pos_neg_edge_detector (
  input logic i_clk,
  input logic i_rstb,
  input logic i_detection_signal,
  output logic o_positive_edge_detected,
  output logic o_negative_edge_detected
);

  logic prev_signal;
  always_ff @(posedge i_clk or negedge i_rstb) begin
    if ((!i_rstb)) begin
      prev_signal <= 0;
    end else begin
      prev_signal <= i_detection_signal;
    end
  end
  always_ff @(posedge i_clk or negedge i_rstb) begin
    if ((!i_rstb)) begin
      o_negative_edge_detected <= 0;
      o_positive_edge_detected <= 0;
    end else begin
      o_positive_edge_detected <= i_detection_signal & ~prev_signal;
      o_negative_edge_detected <= ~i_detection_signal & prev_signal;
    end
  end

endmodule

