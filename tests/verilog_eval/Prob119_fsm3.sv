// domain SysDomain

module TopModule (
  input logic clk,
  input logic areset,
  input logic in_sig,
  output logic out_sig
);

  logic [2-1:0] st;
  always_ff @(posedge clk or posedge areset) begin
    if (areset) begin
      st <= 0;
    end else begin
      if ((st == 0)) begin
        if (in_sig) begin
          st <= 1;
        end
      end else if ((st == 1)) begin
        if ((~in_sig)) begin
          st <= 2;
        end
      end else if ((st == 2)) begin
        if (in_sig) begin
          st <= 3;
        end else begin
          st <= 0;
        end
      end else if (in_sig) begin
        st <= 1;
      end else begin
        st <= 2;
      end
    end
  end
  always_comb begin
    if ((st == 3)) begin
      out_sig = 1;
    end else begin
      out_sig = 0;
    end
  end

endmodule

