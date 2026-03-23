// domain SysDomain

module TopModule (
  input logic clk,
  input logic areset,
  input logic in_sig,
  output logic out_sig
);

  logic st;
  always_ff @(posedge clk or posedge areset) begin
    if (areset) begin
      st <= 1;
    end else begin
      if (st) begin
        if ((~in_sig)) begin
          st <= 0;
        end
      end else if ((~in_sig)) begin
        st <= 1;
      end
    end
  end
  assign out_sig = st;

endmodule

