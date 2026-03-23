// domain SysDomain

module TopModule (
  input logic clk,
  input logic reset_sig,
  input logic [32-1:0] in_sig,
  output logic [32-1:0] out_sig
);

  logic [32-1:0] prev;
  logic [32-1:0] captured;
  always_ff @(posedge clk) begin
    if (reset_sig) begin
      captured <= 0;
    end else begin
      captured <= (captured | (prev & (~in_sig)));
    end
  end
  always_ff @(posedge clk) begin
    prev <= in_sig;
  end
  assign out_sig = captured;

endmodule

