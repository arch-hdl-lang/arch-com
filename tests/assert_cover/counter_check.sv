module CounterCheck (
  input logic clk,
  input logic rst,
  input logic en,
  output logic [7:0] count
);

  logic [7:0] cnt;
  always_ff @(posedge clk) begin
    if (rst) begin
      cnt <= 0;
    end else begin
      if (en) begin
        cnt <= 8'(cnt + 1);
      end
    end
  end
  assign count = cnt;
  // synopsys translate_off
  no_overflow: assert property (@(posedge clk) !(cnt != 0) || en)
    else $fatal(1, "ASSERTION FAILED: CounterCheck.no_overflow");
  saw_max: cover property (@(posedge clk) cnt == 255);
  saw_zero: cover property (@(posedge clk) cnt == 0);
  // synopsys translate_on

endmodule

