module gcd_top #(
  parameter int WIDTH = 4
) (
  input logic clk,
  input logic rst,
  input logic [WIDTH-1:0] A,
  input logic [WIDTH-1:0] B,
  input logic go,
  output logic [WIDTH-1:0] OUT,
  output logic done
);

  logic [WIDTH-1:0] a_r;
  logic [WIDTH-1:0] b_r;
  logic [WIDTH-1:0] out_r;
  logic busy_r;
  logic done_r;
  logic finish_r;
  assign OUT = out_r;
  assign done = done_r;
  always_ff @(posedge clk) begin
    if (rst) begin
      a_r <= 0;
      b_r <= 0;
      busy_r <= 1'b0;
      done_r <= 1'b0;
      finish_r <= 1'b0;
      out_r <= 0;
    end else begin
      done_r <= 1'b0;
      finish_r <= 1'b0;
      if (finish_r) begin
        // Output result and signal done one cycle after match
        out_r <= a_r;
        done_r <= 1'b1;
      end else if (~busy_r) begin
        if (go) begin
          a_r <= A;
          b_r <= B;
          if (A == B) begin
            // Equal at load time: skip to finish next cycle
            finish_r <= 1'b1;
          end else begin
            busy_r <= 1'b1;
          end
        end
      end else if (a_r == b_r) begin
        // Match detected: finish next cycle
        finish_r <= 1'b1;
        busy_r <= 1'b0;
      end else if (a_r > b_r) begin
        a_r <= WIDTH'(a_r - b_r);
      end else begin
        b_r <= WIDTH'(b_r - a_r);
      end
    end
  end

endmodule

