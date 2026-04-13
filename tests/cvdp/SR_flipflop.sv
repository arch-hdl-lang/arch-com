module SR_flipflop (
  input logic i_clk,
  input logic i_rst_b,
  input logic i_S,
  input logic i_R,
  output logic o_Q,
  output logic o_Q_b
);

  logic q;
  logic q_b;
  assign o_Q = q;
  assign o_Q_b = q_b;
  always_ff @(posedge i_clk or negedge i_rst_b) begin
    if ((!i_rst_b)) begin
      q <= 0;
      q_b <= 1;
    end else begin
      if (i_S & i_R) begin
        q <= 0;
        q_b <= 0;
      end else if (i_S) begin
        q <= 1;
        q_b <= 0;
      end else if (i_R) begin
        q <= 0;
        q_b <= 1;
      end
    end
  end

endmodule

