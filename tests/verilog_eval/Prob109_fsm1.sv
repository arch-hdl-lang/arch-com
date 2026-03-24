// domain SysDomain

module TopModule (
  input logic clk,
  input logic areset,
  input logic in,
  output logic out
);

  logic st;
  always_ff @(posedge clk or posedge areset) begin
    if (areset) begin
      st <= 1;
    end else begin
      if (st) begin
        if ((~in)) begin
          st <= 0;
        end
      end else if ((~in)) begin
        st <= 1;
      end
    end
  end
  assign out = st;

endmodule

