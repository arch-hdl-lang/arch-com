module MatchTest (
  input logic [2:0] sel,
  output logic [7:0] out
);

  logic [7:0] val;
  always_comb begin
    val = '0;
    case (sel)
      'b0: val = 1;
      'b1: val = 2;
      'b10: val = 4;
      'b11: val = 8;
      'b100: val = 16;
      'b101: val = 32;
      'b110: val = 64;
      'b111: val = 128;
    endcase
  end
  assign out = val;

endmodule

