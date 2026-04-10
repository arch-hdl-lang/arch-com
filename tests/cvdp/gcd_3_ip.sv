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

  logic [WIDTH-1:0] gcd_ab_out;
  logic [WIDTH-1:0] gcd_bc_out;
  logic gcd_ab_done;
  logic gcd_bc_done;
  logic ab_done_r;
  logic bc_done_r;
  logic [WIDTH-1:0] ab_result_r;
  logic [WIDTH-1:0] bc_result_r;
  logic go_final_r;
  logic busy_r;
  // Mux: use combinational output if just became done, else latched result
  logic [WIDTH-1:0] final_a;
  logic [WIDTH-1:0] final_b;
  assign final_a = gcd_ab_done ? gcd_ab_out : ab_result_r;
  assign final_b = gcd_bc_done ? gcd_bc_out : bc_result_r;
  gcd_top #(.WIDTH(WIDTH)) u_gcd_ab (
    .clk(clk),
    .rst(rst),
    .A(A),
    .B(B),
    .go(go),
    .OUT(gcd_ab_out),
    .done(gcd_ab_done)
  );
  gcd_top #(.WIDTH(WIDTH)) u_gcd_bc (
    .clk(clk),
    .rst(rst),
    .A(B),
    .B(C),
    .go(go),
    .OUT(gcd_bc_out),
    .done(gcd_bc_done)
  );
  // Final GCD uses muxed combinational inputs
  gcd_top #(.WIDTH(WIDTH)) u_gcd_final (
    .clk(clk),
    .rst(rst),
    .A(final_a),
    .B(final_b),
    .go(go_final_r),
    .OUT(OUT),
    .done(done)
  );
  always_ff @(posedge clk) begin
    if (rst) begin
      ab_done_r <= 1'b0;
      ab_result_r <= 0;
      bc_done_r <= 1'b0;
      bc_result_r <= 0;
      busy_r <= 1'b0;
      go_final_r <= 1'b0;
    end else begin
      go_final_r <= 1'b0;
      if (go) begin
        busy_r <= 1'b1;
        ab_done_r <= 1'b0;
        bc_done_r <= 1'b0;
      end
      if (busy_r) begin
        if (gcd_ab_done) begin
          ab_done_r <= 1'b1;
          ab_result_r <= gcd_ab_out;
        end
        if (gcd_bc_done) begin
          bc_done_r <= 1'b1;
          bc_result_r <= gcd_bc_out;
        end
        if ((ab_done_r | gcd_ab_done) & (bc_done_r | gcd_bc_done)) begin
          go_final_r <= 1'b1;
          busy_r <= 1'b0;
          ab_done_r <= 1'b0;
          bc_done_r <= 1'b0;
        end
      end
    end
  end

endmodule

