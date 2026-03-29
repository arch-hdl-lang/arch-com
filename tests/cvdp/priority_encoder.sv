module priority_encoder_8x3 (
  input logic [8-1:0] in,
  output logic [3-1:0] out
);

  always_comb begin
    if (in[7]) begin
      out = 3'd7;
    end else if (in[6]) begin
      out = 3'd6;
    end else if (in[5]) begin
      out = 3'd5;
    end else if (in[4]) begin
      out = 3'd4;
    end else if (in[3]) begin
      out = 3'd3;
    end else if (in[2]) begin
      out = 3'd2;
    end else if (in[1]) begin
      out = 3'd1;
    end else begin
      out = 3'd0;
    end
  end

endmodule

