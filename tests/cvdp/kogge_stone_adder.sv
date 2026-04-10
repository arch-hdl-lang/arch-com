module kogge_stone_adder (
  input logic clk,
  input logic reset,
  input logic [16-1:0] A,
  input logic [16-1:0] B,
  input logic start,
  output logic [17-1:0] Sum,
  output logic done
);

  logic done_reg;
  logic [17-1:0] sum_reg;
  // Kogge-Stone carry computation
  // Initial generate and propagate
  logic g0 [16-1:0];
  logic p0 [16-1:0];
  always_comb begin
    for (int i = 0; i <= 15; i++) begin
      g0[i] = A[i] & B[i];
      p0[i] = A[i] ^ B[i];
    end
  end
  // Stage 1: span 1
  logic g1 [16-1:0];
  logic p1 [16-1:0];
  always_comb begin
    g1[0] = g0[0];
    p1[0] = p0[0];
    for (int i = 1; i <= 15; i++) begin
      g1[i] = g0[i] | p0[i] & g0[i - 1];
      p1[i] = p0[i] & p0[i - 1];
    end
  end
  // Stage 2: span 2
  logic g2 [16-1:0];
  logic p2 [16-1:0];
  always_comb begin
    g2[0] = g1[0];
    p2[0] = p1[0];
    g2[1] = g1[1];
    p2[1] = p1[1];
    for (int i = 2; i <= 15; i++) begin
      g2[i] = g1[i] | p1[i] & g1[i - 2];
      p2[i] = p1[i] & p1[i - 2];
    end
  end
  // Stage 3: span 4
  logic g3 [16-1:0];
  logic p3 [16-1:0];
  always_comb begin
    for (int i = 0; i <= 3; i++) begin
      g3[i] = g2[i];
      p3[i] = p2[i];
    end
    for (int i = 4; i <= 15; i++) begin
      g3[i] = g2[i] | p2[i] & g2[i - 4];
      p3[i] = p2[i] & p2[i - 4];
    end
  end
  // Stage 4: span 8
  logic g4 [16-1:0];
  logic p4 [16-1:0];
  always_comb begin
    for (int i = 0; i <= 7; i++) begin
      g4[i] = g3[i];
      p4[i] = p3[i];
    end
    for (int i = 8; i <= 15; i++) begin
      g4[i] = g3[i] | p3[i] & g3[i - 8];
      p4[i] = p3[i] & p3[i - 8];
    end
  end
  // Compute sum bits: sum[i] = p0[i] ^ carry[i]
  // carry[0] = 0, carry[i] = g4[i-1] for i >= 1
  // carry[16] = g4[15] (MSB carry out)
  logic sum_comb [17-1:0];
  always_comb begin
    sum_comb[0] = p0[0];
    for (int i = 1; i <= 15; i++) begin
      sum_comb[i] = p0[i] ^ g4[i - 1];
    end
    sum_comb[16] = g4[15];
  end
  // Register output on start
  always_ff @(posedge clk) begin
    if (reset) begin
      done_reg <= 0;
      sum_reg <= 0;
    end else begin
      if (start == 1'b1) begin
        for (int i = 0; i <= 16; i++) begin
          sum_reg[i] <= sum_comb[i];
        end
        done_reg <= 1'b1;
      end else begin
        done_reg <= 1'b0;
      end
    end
  end
  assign Sum = sum_reg;
  assign done = done_reg;

endmodule

