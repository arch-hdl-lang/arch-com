// AES Round Constant Generator (standalone module)
module AesRcon (
  input logic clk,
  input logic kld,
  output logic [32-1:0] out_rcon
);

  logic [4-1:0] rcnt = 0;
  always_ff @(posedge clk) begin
    if (kld) begin
      rcnt <= 0;
    end else begin
      rcnt <= 4'((rcnt + 1));
    end
  end
  always_comb begin
    case (rcnt)
      'h0: out_rcon = 'h1000000;
      'h1: out_rcon = 'h2000000;
      'h2: out_rcon = 'h4000000;
      'h3: out_rcon = 'h8000000;
      'h4: out_rcon = 'h10000000;
      'h5: out_rcon = 'h20000000;
      'h6: out_rcon = 'h40000000;
      'h7: out_rcon = 'h80000000;
      'h8: out_rcon = 'h1B000000;
      'h9: out_rcon = 'h36000000;
      default: out_rcon = 'h0;
    endcase
  end

endmodule

