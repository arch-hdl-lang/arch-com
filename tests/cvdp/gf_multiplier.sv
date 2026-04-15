module gf_multiplier (
  input logic [3:0] A,
  input logic [3:0] B,
  output logic [3:0] result
);

  // Unrolled GF(2^4) multiplication with irreducible polynomial x^4+x+1
  // Step 0: start with A, check B[0]
  logic [3:0] p0;
  logic [4:0] a1;
  logic [4:0] a1r;
  // Step 1: check B[1]
  logic [3:0] p1;
  logic [4:0] a2;
  logic [4:0] a2r;
  // Step 2: check B[2]
  logic [3:0] p2;
  logic [4:0] a3;
  logic [4:0] a3r;
  // Step 3: check B[3]
  logic [3:0] p3;
  always_comb begin
    // Step 0: if B[0], XOR A into partial product
    if (B[0] == 1'd1) begin
      p0 = A;
    end else begin
      p0 = 4'd0;
    end
    // Shift A left by 1 (into 5-bit)
    a1 = 5'($unsigned(A)) << 1;
    // Reduce if bit 4 set
    if (a1[4] == 1'd1) begin
      a1r = a1 ^ 5'd19;
    end else begin
      a1r = a1;
    end
    // Step 1: if B[1], XOR shifted A into partial product
    if (B[1] == 1'd1) begin
      p1 = p0 ^ a1r[3:0];
    end else begin
      p1 = p0;
    end
    // Shift a1r left by 1
    a2 = a1r << 1;
    if (a2[4] == 1'd1) begin
      a2r = a2 ^ 5'd19;
    end else begin
      a2r = a2;
    end
    // Step 2: if B[2], XOR
    if (B[2] == 1'd1) begin
      p2 = p1 ^ a2r[3:0];
    end else begin
      p2 = p1;
    end
    // Shift a2r left by 1
    a3 = a2r << 1;
    if (a3[4] == 1'd1) begin
      a3r = a3 ^ 5'd19;
    end else begin
      a3r = a3;
    end
    // Step 3: if B[3], XOR
    if (B[3] == 1'd1) begin
      p3 = p2 ^ a3r[3:0];
    end else begin
      p3 = p2;
    end
    result = p3;
  end

endmodule

