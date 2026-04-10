module lcm_3_ip #(
  parameter int WIDTH = 4,
  localparam int W2 = 2 * WIDTH,
  localparam int W3 = 3 * WIDTH
) (
  input logic clk,
  input logic rst,
  input logic [WIDTH-1:0] A,
  input logic [WIDTH-1:0] B,
  input logic [WIDTH-1:0] C,
  input logic go,
  output logic [W3-1:0] OUT,
  output logic done
);

  // Register the products and go signal (1 cycle delay before GCD)
  logic go_d;
  logic [W2-1:0] ab_r;
  logic [W2-1:0] bc_r;
  logic [W2-1:0] ca_r;
  logic [W3-1:0] abc_r;
  // Output registers
  logic [W3-1:0] out_r;
  logic done_r;
  assign OUT = out_r;
  assign done = done_r;
  // GCD output wires
  logic [W2-1:0] gcd_out;
  logic gcd_done;
  // Instantiate gcd_3_ip to compute GCD(A*B, B*C, C*A)
  gcd_3_ip #(.WIDTH(W2)) u_gcd (
    .clk(clk),
    .rst(rst),
    .A(ab_r),
    .B(bc_r),
    .C(ca_r),
    .go(go_d),
    .OUT(gcd_out),
    .done(gcd_done)
  );
  always_ff @(posedge clk) begin
    if (rst) begin
      ab_r <= 0;
      abc_r <= 0;
      bc_r <= 0;
      ca_r <= 0;
      done_r <= 1'b0;
      go_d <= 1'b0;
      out_r <= 0;
    end else begin
      go_d <= 1'b0;
      done_r <= 1'b0;
      // Register products on go (1 cycle before GCD starts)
      if (go) begin
        ab_r <= W2'(W2'($unsigned(A)) * W2'($unsigned(B)));
        bc_r <= W2'(W2'($unsigned(B)) * W2'($unsigned(C)));
        ca_r <= W2'(W2'($unsigned(C)) * W2'($unsigned(A)));
        abc_r <= W3'(W3'($unsigned(A)) * W3'($unsigned(B)) * W3'($unsigned(C)));
        go_d <= 1'b1;
      end
      // When GCD is done, compute LCM = ABC / GCD (output in same cycle)
      if (gcd_done) begin
        out_r <= abc_r / W3'($unsigned(gcd_out));
        done_r <= 1'b1;
      end
    end
  end

endmodule

