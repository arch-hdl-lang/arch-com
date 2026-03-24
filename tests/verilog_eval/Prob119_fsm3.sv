// domain SysDomain

module TopModule (
  input logic clk,
  input logic areset,
  input logic in,
  output logic out
);

  logic [2-1:0] st;
  always_ff @(posedge clk or posedge areset) begin
    if (areset) begin
      st <= 0;
    end else begin
      if ((st == 0)) begin
        if (in) begin
          st <= 1;
        end
      end else if ((st == 1)) begin
        if ((~in)) begin
          st <= 2;
        end
      end else if ((st == 2)) begin
        if (in) begin
          st <= 3;
        end else begin
          st <= 0;
        end
      end else if (in) begin
        st <= 1;
      end else begin
        st <= 2;
      end
    end
  end
  always_comb begin
    if ((st == 3)) begin
      out = 1;
    end else begin
      out = 0;
    end
  end

endmodule

