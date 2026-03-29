module bcd_adder (
  input logic [4-1:0] a,
  input logic [4-1:0] b,
  input logic cin,
  output logic [4-1:0] sum,
  output logic cout
);

  logic [5-1:0] raw_sum;
  assign raw_sum = 5'(5'($unsigned(a)) + 5'($unsigned(b)) + 5'($unsigned(cin)));
  logic needs_correction;
  assign needs_correction = raw_sum > 9;
  logic [5-1:0] corrected;
  assign corrected = 5'(raw_sum + 6);
  always_comb begin
    if (needs_correction) begin
      sum = corrected[3:0];
      cout = 1'b1;
    end else begin
      sum = raw_sum[3:0];
      cout = raw_sum[4:4] == 1;
    end
  end

endmodule

