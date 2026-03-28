// VerilogEval Prob080: Down counter timer
module TopModule (
  input logic clk,
  input logic load,
  input logic [10-1:0] data,
  output logic tc
);

  logic [10-1:0] count_val = 0;
  always_ff @(posedge clk) begin
    if (load == 1) begin
      count_val <= data;
    end else if (count_val != 0) begin
      count_val <= 10'(count_val - 1);
    end
  end
  assign tc = count_val == 0;

endmodule

