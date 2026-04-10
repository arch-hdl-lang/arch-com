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
  assign OUT = out_r;
  assign done = done_r;
  always_ff @(posedge clk) begin
    if (rst) begin
      a_r <= 0;
      b_r <= 0;
      busy_r <= 1'b0;
      done_r <= 1'b0;
      out_r <= 0;
    end else begin
      done_r <= 1'b0;
      if (~busy_r) begin
        if (go) begin
          a_r <= A;
          b_r <= B;
          out_r <= 0;
          busy_r <= 1'b1;
        end
      end else if (a_r == b_r) begin
        out_r <= a_r;
        busy_r <= 1'b0;
        done_r <= 1'b1;
      end else if (a_r > b_r) begin
        a_r <= WIDTH'(a_r - b_r);
      end else begin
        b_r <= WIDTH'(b_r - a_r);
      end
    end
  end

endmodule

module gcd_3_ip #(
  parameter int WIDTH = 4
) (
  input logic clk,
  input logic rst,
  input logic [WIDTH-1:0] A,
  input logic [WIDTH-1:0] B,
  input logic [WIDTH-1:0] C,
  input logic go,
  output logic [WIDTH-1:0] OUT,
  output logic done
);

  // Wires for intermediate GCD results
  logic [WIDTH-1:0] gcd_ab_out;
  logic [WIDTH-1:0] gcd_bc_out;
  logic gcd_ab_done;
  logic gcd_bc_done;
  logic [WIDTH-1:0] gcd_final_out;
  logic gcd_final_done;
  // Registers to latch intermediate results and track readiness
  logic ab_done_r;
  logic bc_done_r;
  logic [WIDTH-1:0] ab_result_r;
  logic [WIDTH-1:0] bc_result_r;
  logic go_final_r;
  logic busy_r;
  logic [WIDTH-1:0] out_r;
  logic done_r;
  assign OUT = out_r;
  assign done = done_r;
  // First GCD: compute GCD(A, B)
  gcd_top #(.WIDTH(WIDTH)) u_gcd_ab (
    .clk(clk),
    .rst(rst),
    .A(A),
    .B(B),
    .go(go),
    .OUT(gcd_ab_out),
    .done(gcd_ab_done)
  );
  // Second GCD: compute GCD(B, C)
  gcd_top #(.WIDTH(WIDTH)) u_gcd_bc (
    .clk(clk),
    .rst(rst),
    .A(B),
    .B(C),
    .go(go),
    .OUT(gcd_bc_out),
    .done(gcd_bc_done)
  );
  // Third GCD: compute GCD(result_ab, result_bc)
  gcd_top #(.WIDTH(WIDTH)) u_gcd_final (
    .clk(clk),
    .rst(rst),
    .A(ab_result_r),
    .B(bc_result_r),
    .go(go_final_r),
    .OUT(gcd_final_out),
    .done(gcd_final_done)
  );
  always_ff @(posedge clk) begin
    if (rst) begin
      ab_done_r <= 1'b0;
      ab_result_r <= 0;
      bc_done_r <= 1'b0;
      bc_result_r <= 0;
      busy_r <= 1'b0;
      done_r <= 1'b0;
      go_final_r <= 1'b0;
      out_r <= 0;
    end else begin
      done_r <= 1'b0;
      go_final_r <= 1'b0;
      if (go) begin
        busy_r <= 1'b1;
        ab_done_r <= 1'b0;
        bc_done_r <= 1'b0;
      end
      if (busy_r) begin
        // Latch GCD(A,B) result when ready
        if (gcd_ab_done) begin
          ab_done_r <= 1'b1;
          ab_result_r <= gcd_ab_out;
        end
        // Latch GCD(B,C) result when ready
        if (gcd_bc_done) begin
          bc_done_r <= 1'b1;
          bc_result_r <= gcd_bc_out;
        end
        // When both intermediate results are ready, start final GCD
        if (ab_done_r & bc_done_r) begin
          go_final_r <= 1'b1;
          busy_r <= 1'b0;
          ab_done_r <= 1'b0;
          bc_done_r <= 1'b0;
        end
        // Also handle case where both finish on the same cycle
        if (gcd_ab_done & bc_done_r) begin
          go_final_r <= 1'b1;
          ab_result_r <= gcd_ab_out;
          busy_r <= 1'b0;
          ab_done_r <= 1'b0;
          bc_done_r <= 1'b0;
        end
        if (ab_done_r & gcd_bc_done) begin
          go_final_r <= 1'b1;
          bc_result_r <= gcd_bc_out;
          busy_r <= 1'b0;
          ab_done_r <= 1'b0;
          bc_done_r <= 1'b0;
        end
        if (gcd_ab_done & gcd_bc_done) begin
          go_final_r <= 1'b1;
          ab_result_r <= gcd_ab_out;
          bc_result_r <= gcd_bc_out;
          busy_r <= 1'b0;
          ab_done_r <= 1'b0;
          bc_done_r <= 1'b0;
        end
      end
      // Final GCD done
      if (gcd_final_done) begin
        out_r <= gcd_final_out;
        done_r <= 1'b1;
      end
    end
  end

endmodule

