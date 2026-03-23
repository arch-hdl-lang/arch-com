// domain SysDomain

module TopModule (
  input logic clk,
  input logic reset_sig,
  input logic in_sig,
  output logic done
);

  logic [4-1:0] st;
  always_ff @(posedge clk) begin
    if (reset_sig) begin
      st <= 8;
    end else begin
      if ((st == 8)) begin
        if ((~in_sig)) begin
          st <= 0;
        end
      end else if ((st == 0)) begin
        st <= 1;
      end else if ((st == 1)) begin
        st <= 2;
      end else if ((st == 2)) begin
        st <= 3;
      end else if ((st == 3)) begin
        st <= 4;
      end else if ((st == 4)) begin
        st <= 5;
      end else if ((st == 5)) begin
        st <= 6;
      end else if ((st == 6)) begin
        st <= 7;
      end else if ((st == 7)) begin
        st <= 9;
      end else if ((st == 9)) begin
        if (in_sig) begin
          st <= 10;
        end else begin
          st <= 11;
        end
      end else if ((st == 10)) begin
        if (in_sig) begin
          st <= 8;
        end else begin
          st <= 0;
        end
      end else if ((st == 11)) begin
        if (in_sig) begin
          st <= 8;
        end
      end
    end
  end
  always_comb begin
    if ((st == 10)) begin
      done = 1;
    end else begin
      done = 0;
    end
  end

endmodule

