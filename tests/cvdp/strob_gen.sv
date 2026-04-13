module strob_gen #(
  parameter int CLOCK_HZ = 10000000,
  parameter int PERIOD_US = 100,
  parameter int DELAY = CLOCK_HZ * PERIOD_US / 1000000 - 1
) (
  input logic clk,
  input logic nrst,
  input logic enable,
  output logic strobe_o
);

  // DELAY = (CLOCK_HZ * PERIOD_US / 1_000_000) - 1
  logic [39:0] cnt;
  always_ff @(posedge clk or negedge nrst) begin
    if ((!nrst)) begin
      cnt <= 40'($unsigned(DELAY));
      strobe_o <= 1'b0;
    end else begin
      if (cnt == 0) begin
        if (enable == 1'b1) begin
          strobe_o <= 1'b1;
        end else begin
          strobe_o <= 1'b0;
        end
        cnt <= 40'($unsigned(DELAY));
      end else begin
        strobe_o <= 1'b0;
        if (enable == 1'b1) begin
          cnt <= 40'(cnt - 1);
        end else begin
          cnt <= 40'($unsigned(DELAY));
        end
      end
    end
  end

endmodule

