module gcd_controlpath #(
  parameter int WIDTH = 4
) (
  input logic clk,
  input logic rst,
  input logic go,
  input logic equal,
  input logic greater_than,
  output logic [2-1:0] controlpath_state,
  output logic done
);

  logic [2-1:0] state_r = 0;
  logic done_r = 1'b0;
  logic [2-1:0] next_state;
  always_comb begin
    if (state_r == 2'd0) begin
      if (go) begin
        if (equal) begin
          next_state = 2'd1;
        end else if (greater_than) begin
          next_state = 2'd2;
        end else begin
          next_state = 2'd3;
        end
      end else begin
        next_state = 2'd0;
      end
    end else if (state_r == 2'd1) begin
      next_state = 2'd0;
    end else if (equal) begin
      next_state = 2'd1;
    end else if (greater_than) begin
      next_state = 2'd2;
    end else begin
      next_state = 2'd3;
    end
    controlpath_state = state_r;
    done = done_r;
  end
  always_ff @(posedge clk) begin
    if (rst) begin
      done_r <= 1'b0;
      state_r <= 0;
    end else begin
      state_r <= next_state;
      done_r <= state_r == 2'd1;
    end
  end

endmodule

