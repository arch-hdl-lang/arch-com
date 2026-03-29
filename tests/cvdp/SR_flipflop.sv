module SR_flipflop (
  input logic i_clk,
  input logic i_rst_b,
  input logic i_S,
  input logic i_R,
  output logic o_Q,
  output logic o_Q_b
);

  always_ff @(posedge i_clk or negedge i_rst_b) begin
    if ((!i_rst_b)) begin
      o_Q <= 1'b0;
      o_Q_b <= 1'b1;
    end else begin
      if (i_S & i_R) begin
        // Invalid state: both outputs low
        o_Q <= 1'b0;
        o_Q_b <= 1'b0;
      end else if (i_S) begin
        o_Q <= 1'b1;
        o_Q_b <= 1'b0;
      end else if (i_R) begin
        o_Q <= 1'b0;
        o_Q_b <= 1'b1;
      end
      // else: hold
    end
  end

endmodule

