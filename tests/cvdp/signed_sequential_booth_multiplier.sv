module signed_sequential_booth_multiplier #(
  parameter int WIDTH = 8,
  localparam int HALF = WIDTH / 2,
  localparam int DW = 2 * WIDTH
) (
  input logic clk,
  input logic rst,
  input logic start,
  input logic signed [WIDTH-1:0] A,
  input logic signed [WIDTH-1:0] B,
  output logic signed [2 * WIDTH-1:0] result,
  output logic done
);

  // State encoding: 0=IDLE, 1=COMPUTE, 2=DONE
  logic [1:0] st;
  // Datapath registers
  logic signed [DW-1:0] accum_r;
  logic signed [DW-1:0] mcand_r;
  logic [WIDTH + 1-1:0] mplier_r;
  logic [7:0] step_r;
  logic done_r;
  // Current 3-bit Booth group from bottom of mplier_r
  logic [2:0] grp;
  assign grp = mplier_r[2:0];
  // Booth partial product selection
  logic signed [DW + 1-1:0] m_pos;
  assign m_pos = {{(DW + 1-$bits(mcand_r)){mcand_r[$bits(mcand_r)-1]}}, mcand_r};
  logic signed [DW + 1-1:0] m2_pos;
  assign m2_pos = {{(DW + 1-$bits(mcand_r)){mcand_r[$bits(mcand_r)-1]}}, mcand_r} << 1;
  logic signed [DW + 1-1:0] m_neg;
  assign m_neg = (DW + 1)'(0 - {{(DW + 1-$bits(mcand_r)){mcand_r[$bits(mcand_r)-1]}}, mcand_r});
  logic signed [DW + 1-1:0] m2_neg;
  assign m2_neg = (DW + 1)'(0 - m2_pos);
  logic signed [DW + 1-1:0] pp_val;
  logic signed [DW + 1-1:0] accum_ext;
  assign accum_ext = {{(DW + 1-$bits(accum_r)){accum_r[$bits(accum_r)-1]}}, accum_r};
  logic signed [DW + 1-1:0] sum;
  assign sum = (DW + 1)'(accum_ext + pp_val);
  // Step thresholds
  logic [7:0] last_step_v;
  assign last_step_v = 8'($unsigned(HALF + 2));
  logic [7:0] half_val_v;
  assign half_val_v = 8'($unsigned(HALF));
  assign result = accum_r;
  assign done = done_r;
  assign pp_val = (grp == 1) | (grp == 2) ? m_pos : grp == 3 ? m2_pos : grp == 4 ? m2_neg : (grp == 5) | (grp == 6) ? m_neg : m_pos ^ m_pos;
  always_ff @(posedge clk or posedge rst) begin
    if (rst) begin
      accum_r <= 0;
      done_r <= 0;
      mcand_r <= 0;
      mplier_r <= 0;
      st <= 0;
      step_r <= 0;
    end else begin
      if (st == 0) begin
        // IDLE
        done_r <= 1'b0;
        if (start) begin
          accum_r <= 0;
          mcand_r <= {{(DW-$bits(A)){A[$bits(A)-1]}}, A};
          mplier_r <= {$unsigned(B), 1'd0};
          step_r <= 0;
          st <= 1;
        end
      end else if (st == 1) begin
        // COMPUTE
        if (step_r < half_val_v) begin
          accum_r <= DW'(sum);
          mplier_r <= mplier_r >> 2;
          mcand_r <= mcand_r << 2;
        end
        step_r <= 8'(step_r + 1);
        if (step_r == last_step_v) begin
          st <= 2;
          done_r <= 1'b1;
        end
      end else begin
        // DONE
        done_r <= 1'b0;
        st <= 0;
      end
    end
  end

endmodule

