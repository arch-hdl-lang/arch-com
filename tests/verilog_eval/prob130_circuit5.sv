Wrote tests/verilog_eval/prob130_circuit5.sv

module TopModule (
  input logic [4-1:0] a,
  input logic [4-1:0] b,
  input logic [4-1:0] c,
  input logic [4-1:0] d,
  input logic [4-1:0] e,
  output logic [4-1:0] q
);

  always_comb begin
    if (c == 0) begin
      q = b;
    end else if (c == 1) begin
      q = e;
    end else if (c == 2) begin
      q = a;
    end else if (c == 3) begin
      q = d;
    end else begin
      q = 4'd15;
    end
  end

endmodule

