module pipelined_modified_booth_multiplier (
  input logic clk,
  input logic rst,
  input logic start,
  input logic signed [16-1:0] X,
  input logic signed [16-1:0] Y,
  output logic signed [32-1:0] result,
  output logic done
);

  // Stage 1: latch inputs
  logic signed [16-1:0] s1_x;
  logic signed [16-1:0] s1_y;
  logic s1_v;
  // Stage 2: compute product
  logic signed [32-1:0] s2_prod;
  logic s2_v;
  // Stage 3: pass through
  logic signed [32-1:0] s3_prod;
  logic s3_v;
  // Stage 4: pass through
  logic signed [32-1:0] s4_prod;
  logic s4_v;
  // Stage 5: output
  logic signed [32-1:0] s5_prod;
  logic s5_v;
  // Multiply in combinational logic from stage 1 regs
  logic signed [32-1:0] prod;
  assign prod = s1_x * s1_y;
  assign result = s5_prod;
  assign done = s5_v;
  always_ff @(posedge clk or posedge rst) begin
    if (rst) begin
      s1_v <= 0;
      s1_x <= 0;
      s1_y <= 0;
      s2_prod <= 0;
      s2_v <= 0;
      s3_prod <= 0;
      s3_v <= 0;
      s4_prod <= 0;
      s4_v <= 0;
      s5_prod <= 0;
      s5_v <= 0;
    end else begin
      // Stage 1: latch inputs
      s1_x <= X;
      s1_y <= Y;
      s1_v <= start;
      // Stage 2: multiply
      s2_prod <= prod;
      s2_v <= s1_v;
      // Stage 3: pass
      s3_prod <= s2_prod;
      s3_v <= s2_v;
      // Stage 4: pass
      s4_prod <= s3_prod;
      s4_v <= s3_v;
      // Stage 5: pass
      s5_prod <= s4_prod;
      s5_v <= s4_v;
    end
  end

endmodule

