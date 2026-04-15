module bcd_to_excess_3 (
  input logic [3:0] bcd,
  output logic [3:0] excess3,
  output logic error
);

  always_comb begin
    if (bcd > 4'd9) begin
      excess3 = 4'd0;
      error = 1'd1;
    end else begin
      excess3 = 4'(bcd + 4'd3);
      error = 1'd0;
    end
  end

endmodule

