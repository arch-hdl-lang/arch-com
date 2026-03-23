// domain SysDomain

module TopModule (
  input logic clk,
  input logic reset_sig,
  input logic w,
  output logic z
);

  logic [3-1:0] st;
  always_ff @(posedge clk) begin
    if (reset_sig) begin
      st <= 0;
    end else begin
      if ((st == 0)) begin
        if (w) begin
          st <= 0;
        end else begin
          st <= 1;
        end
      end else if ((st == 1)) begin
        if (w) begin
          st <= 3;
        end else begin
          st <= 2;
        end
      end else if ((st == 2)) begin
        if (w) begin
          st <= 3;
        end else begin
          st <= 4;
        end
      end else if ((st == 3)) begin
        if (w) begin
          st <= 0;
        end else begin
          st <= 5;
        end
      end else if ((st == 4)) begin
        if (w) begin
          st <= 3;
        end else begin
          st <= 4;
        end
      end else if (w) begin
        st <= 3;
      end else begin
        st <= 2;
      end
    end
  end
  always_comb begin
    if (((st == 4) | (st == 5))) begin
      z = 1;
    end else begin
      z = 0;
    end
  end

endmodule

