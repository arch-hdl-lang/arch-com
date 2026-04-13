module priority_encoder_8x3 (
  input logic [7:0] in,
  output logic [2:0] out
);

  logic [2:0] result;
  always_comb begin
    if (in[7]) begin
      result = 3'd7;
    end else if (in[6]) begin
      result = 3'd6;
    end else if (in[5]) begin
      result = 3'd5;
    end else if (in[4]) begin
      result = 3'd4;
    end else if (in[3]) begin
      result = 3'd3;
    end else if (in[2]) begin
      result = 3'd2;
    end else if (in[1]) begin
      result = 3'd1;
    end else begin
      result = 3'd0;
    end
    out = result;
  end

endmodule

