// domain SysDomain

module TopModule (
  input logic clk,
  input logic reset_sig,
  input logic j,
  input logic k,
  output logic out_sig
);

  logic st;
  always_ff @(posedge clk) begin
    if (reset_sig) begin
      st <= 0;
    end else begin
      if ((~st)) begin
        if (j) begin
          st <= 1;
        end
      end else if (k) begin
        st <= 0;
      end
    end
  end
  assign out_sig = st;

endmodule

