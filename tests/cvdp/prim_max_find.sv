// Find maximum value and its index among NumSrc inputs.
// Uses a registered binary tree: level 1 (leaf) registers latch inputs,
// then bubble up through tree levels, one register stage per level.
// For NumSrc=8, Width=8: SrcWidth=3, NumLevels=2, result latency = NumLevels+1 = 3 cycles.
module prim_max_find #(
  parameter int NumSrc = 8,
  parameter int Width = 8,
  parameter int SrcWidth = 3
) (
  input logic clk_i,
  input logic rst_ni,
  input logic [Width * NumSrc-1:0] values_i,
  input logic [NumSrc-1:0] valid_i,
  output logic [Width-1:0] max_value_o,
  output logic [SrcWidth-1:0] max_idx_o,
  output logic max_valid_o
);

  // Extract individual values from the flattened input
  logic [Width-1:0] v0;
  assign v0 = values_i[1 * Width - 1:0 * Width];
  logic [Width-1:0] v1;
  assign v1 = values_i[2 * Width - 1:1 * Width];
  logic [Width-1:0] v2;
  assign v2 = values_i[3 * Width - 1:2 * Width];
  logic [Width-1:0] v3;
  assign v3 = values_i[4 * Width - 1:3 * Width];
  logic [Width-1:0] v4;
  assign v4 = values_i[5 * Width - 1:4 * Width];
  logic [Width-1:0] v5;
  assign v5 = values_i[6 * Width - 1:5 * Width];
  logic [Width-1:0] v6;
  assign v6 = values_i[7 * Width - 1:6 * Width];
  logic [Width-1:0] v7;
  assign v7 = values_i[8 * Width - 1:7 * Width];
  logic vld0;
  assign vld0 = valid_i[0:0] != 0;
  logic vld1;
  assign vld1 = valid_i[1:1] != 0;
  logic vld2;
  assign vld2 = valid_i[2:2] != 0;
  logic vld3;
  assign vld3 = valid_i[3:3] != 0;
  logic vld4;
  assign vld4 = valid_i[4:4] != 0;
  logic vld5;
  assign vld5 = valid_i[5:5] != 0;
  logic vld6;
  assign vld6 = valid_i[6:6] != 0;
  logic vld7;
  assign vld7 = valid_i[7:7] != 0;
  // Level 2 (leaf): register all 8 inputs
  logic [Width-1:0] l2_val0;
  logic [Width-1:0] l2_val1;
  logic [Width-1:0] l2_val2;
  logic [Width-1:0] l2_val3;
  logic [Width-1:0] l2_val4;
  logic [Width-1:0] l2_val5;
  logic [Width-1:0] l2_val6;
  logic [Width-1:0] l2_val7;
  logic [SrcWidth-1:0] l2_idx0;
  logic [SrcWidth-1:0] l2_idx1;
  logic [SrcWidth-1:0] l2_idx2;
  logic [SrcWidth-1:0] l2_idx3;
  logic [SrcWidth-1:0] l2_idx4;
  logic [SrcWidth-1:0] l2_idx5;
  logic [SrcWidth-1:0] l2_idx6;
  logic [SrcWidth-1:0] l2_idx7;
  logic l2_vld0;
  logic l2_vld1;
  logic l2_vld2;
  logic l2_vld3;
  logic l2_vld4;
  logic l2_vld5;
  logic l2_vld6;
  logic l2_vld7;
  always_ff @(posedge clk_i or negedge rst_ni) begin
    if ((!rst_ni)) begin
      l2_idx0 <= 0;
      l2_idx1 <= 0;
      l2_idx2 <= 0;
      l2_idx3 <= 0;
      l2_idx4 <= 0;
      l2_idx5 <= 0;
      l2_idx6 <= 0;
      l2_idx7 <= 0;
      l2_val0 <= 0;
      l2_val1 <= 0;
      l2_val2 <= 0;
      l2_val3 <= 0;
      l2_val4 <= 0;
      l2_val5 <= 0;
      l2_val6 <= 0;
      l2_val7 <= 0;
      l2_vld0 <= 1'b0;
      l2_vld1 <= 1'b0;
      l2_vld2 <= 1'b0;
      l2_vld3 <= 1'b0;
      l2_vld4 <= 1'b0;
      l2_vld5 <= 1'b0;
      l2_vld6 <= 1'b0;
      l2_vld7 <= 1'b0;
    end else begin
      l2_val0 <= v0;
      l2_idx0 <= 0;
      l2_vld0 <= vld0;
      l2_val1 <= v1;
      l2_idx1 <= 1;
      l2_vld1 <= vld1;
      l2_val2 <= v2;
      l2_idx2 <= 2;
      l2_vld2 <= vld2;
      l2_val3 <= v3;
      l2_idx3 <= 3;
      l2_vld3 <= vld3;
      l2_val4 <= v4;
      l2_idx4 <= 4;
      l2_vld4 <= vld4;
      l2_val5 <= v5;
      l2_idx5 <= 5;
      l2_vld5 <= vld5;
      l2_val6 <= v6;
      l2_idx6 <= 6;
      l2_vld6 <= vld6;
      l2_val7 <= v7;
      l2_idx7 <= 7;
      l2_vld7 <= vld7;
    end
  end
  // Helper wires for Level 1 comparisons (pairs from Level 2)
  logic [Width-1:0] m01_val;
  logic [SrcWidth-1:0] m01_idx;
  logic m01_vld;
  logic [Width-1:0] m23_val;
  logic [SrcWidth-1:0] m23_idx;
  logic m23_vld;
  logic [Width-1:0] m45_val;
  logic [SrcWidth-1:0] m45_idx;
  logic m45_vld;
  logic [Width-1:0] m67_val;
  logic [SrcWidth-1:0] m67_idx;
  logic m67_vld;
  always_comb begin
    // Compare pair (0,1)
    if (l2_vld0 == 1'b0 & l2_vld1) begin
      m01_val = l2_val1;
      m01_idx = l2_idx1;
      m01_vld = 1'b1;
    end else if (l2_vld0 & l2_vld1 == 1'b0) begin
      m01_val = l2_val0;
      m01_idx = l2_idx0;
      m01_vld = 1'b1;
    end else if (l2_vld0 & l2_vld1) begin
      if (l2_val0 >= l2_val1) begin
        m01_val = l2_val0;
        m01_idx = l2_idx0;
      end else begin
        m01_val = l2_val1;
        m01_idx = l2_idx1;
      end
      m01_vld = 1'b1;
    end else begin
      m01_val = 0;
      m01_idx = 0;
      m01_vld = 1'b0;
    end
    // Compare pair (2,3)
    if (l2_vld2 == 1'b0 & l2_vld3) begin
      m23_val = l2_val3;
      m23_idx = l2_idx3;
      m23_vld = 1'b1;
    end else if (l2_vld2 & l2_vld3 == 1'b0) begin
      m23_val = l2_val2;
      m23_idx = l2_idx2;
      m23_vld = 1'b1;
    end else if (l2_vld2 & l2_vld3) begin
      if (l2_val2 >= l2_val3) begin
        m23_val = l2_val2;
        m23_idx = l2_idx2;
      end else begin
        m23_val = l2_val3;
        m23_idx = l2_idx3;
      end
      m23_vld = 1'b1;
    end else begin
      m23_val = 0;
      m23_idx = 0;
      m23_vld = 1'b0;
    end
    // Compare pair (4,5)
    if (l2_vld4 == 1'b0 & l2_vld5) begin
      m45_val = l2_val5;
      m45_idx = l2_idx5;
      m45_vld = 1'b1;
    end else if (l2_vld4 & l2_vld5 == 1'b0) begin
      m45_val = l2_val4;
      m45_idx = l2_idx4;
      m45_vld = 1'b1;
    end else if (l2_vld4 & l2_vld5) begin
      if (l2_val4 >= l2_val5) begin
        m45_val = l2_val4;
        m45_idx = l2_idx4;
      end else begin
        m45_val = l2_val5;
        m45_idx = l2_idx5;
      end
      m45_vld = 1'b1;
    end else begin
      m45_val = 0;
      m45_idx = 0;
      m45_vld = 1'b0;
    end
    // Compare pair (6,7)
    if (l2_vld6 == 1'b0 & l2_vld7) begin
      m67_val = l2_val7;
      m67_idx = l2_idx7;
      m67_vld = 1'b1;
    end else if (l2_vld6 & l2_vld7 == 1'b0) begin
      m67_val = l2_val6;
      m67_idx = l2_idx6;
      m67_vld = 1'b1;
    end else if (l2_vld6 & l2_vld7) begin
      if (l2_val6 >= l2_val7) begin
        m67_val = l2_val6;
        m67_idx = l2_idx6;
      end else begin
        m67_val = l2_val7;
        m67_idx = l2_idx7;
      end
      m67_vld = 1'b1;
    end else begin
      m67_val = 0;
      m67_idx = 0;
      m67_vld = 1'b0;
    end
  end
  // Level 1 registers
  logic [Width-1:0] l1_val01;
  logic [SrcWidth-1:0] l1_idx01;
  logic l1_vld01;
  logic [Width-1:0] l1_val23;
  logic [SrcWidth-1:0] l1_idx23;
  logic l1_vld23;
  logic [Width-1:0] l1_val45;
  logic [SrcWidth-1:0] l1_idx45;
  logic l1_vld45;
  logic [Width-1:0] l1_val67;
  logic [SrcWidth-1:0] l1_idx67;
  logic l1_vld67;
  always_ff @(posedge clk_i or negedge rst_ni) begin
    if ((!rst_ni)) begin
      l1_idx01 <= 0;
      l1_idx23 <= 0;
      l1_idx45 <= 0;
      l1_idx67 <= 0;
      l1_val01 <= 0;
      l1_val23 <= 0;
      l1_val45 <= 0;
      l1_val67 <= 0;
      l1_vld01 <= 1'b0;
      l1_vld23 <= 1'b0;
      l1_vld45 <= 1'b0;
      l1_vld67 <= 1'b0;
    end else begin
      l1_val01 <= m01_val;
      l1_idx01 <= m01_idx;
      l1_vld01 <= m01_vld;
      l1_val23 <= m23_val;
      l1_idx23 <= m23_idx;
      l1_vld23 <= m23_vld;
      l1_val45 <= m45_val;
      l1_idx45 <= m45_idx;
      l1_vld45 <= m45_vld;
      l1_val67 <= m67_val;
      l1_idx67 <= m67_idx;
      l1_vld67 <= m67_vld;
    end
  end
  // Level 0 comparisons
  logic [Width-1:0] m0123_val;
  logic [SrcWidth-1:0] m0123_idx;
  logic m0123_vld;
  logic [Width-1:0] m4567_val;
  logic [SrcWidth-1:0] m4567_idx;
  logic m4567_vld;
  always_comb begin
    // Compare (01, 23)
    if (l1_vld01 == 1'b0 & l1_vld23) begin
      m0123_val = l1_val23;
      m0123_idx = l1_idx23;
      m0123_vld = 1'b1;
    end else if (l1_vld01 & l1_vld23 == 1'b0) begin
      m0123_val = l1_val01;
      m0123_idx = l1_idx01;
      m0123_vld = 1'b1;
    end else if (l1_vld01 & l1_vld23) begin
      if (l1_val01 >= l1_val23) begin
        m0123_val = l1_val01;
        m0123_idx = l1_idx01;
      end else begin
        m0123_val = l1_val23;
        m0123_idx = l1_idx23;
      end
      m0123_vld = 1'b1;
    end else begin
      m0123_val = 0;
      m0123_idx = 0;
      m0123_vld = 1'b0;
    end
    // Compare (45, 67)
    if (l1_vld45 == 1'b0 & l1_vld67) begin
      m4567_val = l1_val67;
      m4567_idx = l1_idx67;
      m4567_vld = 1'b1;
    end else if (l1_vld45 & l1_vld67 == 1'b0) begin
      m4567_val = l1_val45;
      m4567_idx = l1_idx45;
      m4567_vld = 1'b1;
    end else if (l1_vld45 & l1_vld67) begin
      if (l1_val45 >= l1_val67) begin
        m4567_val = l1_val45;
        m4567_idx = l1_idx45;
      end else begin
        m4567_val = l1_val67;
        m4567_idx = l1_idx67;
      end
      m4567_vld = 1'b1;
    end else begin
      m4567_val = 0;
      m4567_idx = 0;
      m4567_vld = 1'b0;
    end
  end
  // Root (level 0) registers
  logic [Width-1:0] l0_val;
  logic [SrcWidth-1:0] l0_idx;
  logic l0_vld;
  logic [Width-1:0] root_val;
  logic [SrcWidth-1:0] root_idx;
  logic root_vld;
  always_comb begin
    if (m0123_vld == 1'b0 & m4567_vld) begin
      root_val = m4567_val;
      root_idx = m4567_idx;
      root_vld = 1'b1;
    end else if (m0123_vld & m4567_vld == 1'b0) begin
      root_val = m0123_val;
      root_idx = m0123_idx;
      root_vld = 1'b1;
    end else if (m0123_vld & m4567_vld) begin
      if (m0123_val >= m4567_val) begin
        root_val = m0123_val;
        root_idx = m0123_idx;
      end else begin
        root_val = m4567_val;
        root_idx = m4567_idx;
      end
      root_vld = 1'b1;
    end else begin
      root_val = 0;
      root_idx = 0;
      root_vld = 1'b0;
    end
  end
  always_ff @(posedge clk_i or negedge rst_ni) begin
    if ((!rst_ni)) begin
      l0_idx <= 0;
      l0_val <= 0;
      l0_vld <= 1'b0;
    end else begin
      l0_val <= root_val;
      l0_idx <= root_idx;
      l0_vld <= root_vld;
    end
  end
  assign max_value_o = l0_val;
  assign max_idx_o = l0_idx;
  assign max_valid_o = l0_vld;

endmodule

