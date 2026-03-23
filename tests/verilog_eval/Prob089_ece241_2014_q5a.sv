// domain SysDomain

module TopModule (
  input logic clk,
  input logic areset,
  input logic x,
  output logic z
);

  logic [2-1:0] st;
  always_ff @(posedge clk or posedge areset) begin
    if (areset) begin
      st <= 0;
    end else begin
      if ((st == 0)) begin
        if (x) begin
          st <= 1;
        end
      end else if ((st == 1)) begin
        if (x) begin
          st <= 3;
        end else begin
          st <= 2;
        end
      end else if ((st == 2)) begin
        if (x) begin
          st <= 3;
        end
      end else if ((st == 3)) begin
        if ((~x)) begin
          st <= 2;
        end
      end
    end
  end
  always_comb begin
    if (((st == 1) | (st == 2))) begin
      z = 1;
    end else begin
      z = 0;
    end
  end

endmodule

