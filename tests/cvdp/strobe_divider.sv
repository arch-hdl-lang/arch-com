module strobe_divider #(
  parameter int MaxRatio_g = 10,
  parameter int Latency_g = 1
) (
  input logic Clk,
  input logic Rst,
  input logic [$clog2(MaxRatio_g)-1:0] In_Ratio,
  input logic In_Valid,
  output logic Out_Valid,
  input logic Out_Ready
);

  logic [$clog2(MaxRatio_g)-1:0] r_Count;
  logic r_OutValid;
  logic [$clog2(MaxRatio_g)-1:0] r_next_Count;
  logic r_next_OutValid;
  logic OutValid_v;
  logic count_match;
  // Precompute count match to avoid && in if conditions
  logic [$clog2(MaxRatio_g)-1:0] ratio_minus1;
  assign ratio_minus1 = $clog2(MaxRatio_g)'(In_Ratio - 1);
  assign count_match = r_Count == ratio_minus1;
  always_comb begin
    // Default: hold current state
    r_next_Count = r_Count;
    r_next_OutValid = r_OutValid;
    // Counter logic for division ratio
    if (In_Valid == 1'b1) begin
      if (In_Ratio == 0) begin
        r_next_Count = 0;
      end else if (count_match == 1'b1) begin
        r_next_Count = 0;
      end else begin
        r_next_Count = $clog2(MaxRatio_g)'(r_Count + 1);
      end
    end
    // Latency handling
    if (Latency_g == 0) begin
      if (In_Ratio == 0) begin
        OutValid_v = In_Valid;
      end else if (In_Valid == 1'b1) begin
        if (count_match == 1'b1) begin
          OutValid_v = 1'b1;
        end else begin
          OutValid_v = 1'b0;
        end
      end else begin
        OutValid_v = 1'b0;
      end
    end else begin
      OutValid_v = r_OutValid;
    end
    // Output pulse generation for registered path
    if (In_Ratio == 0) begin
      r_next_OutValid = In_Valid;
    end else if (In_Valid == 1'b1) begin
      if (count_match == 1'b1) begin
        r_next_OutValid = 1'b1;
      end
    end else if (Out_Ready == 1'b1) begin
      if (r_OutValid == 1'b1) begin
        r_next_OutValid = 1'b0;
      end
    end
    // Output ready handshake
    if (Latency_g == 0) begin
      if (Out_Ready == 1'b1) begin
        Out_Valid = OutValid_v;
      end else if (r_OutValid == 1'b0) begin
        Out_Valid = OutValid_v;
      end else begin
        Out_Valid = 1'b1;
      end
    end else begin
      Out_Valid = OutValid_v;
    end
  end
  always_ff @(posedge Clk) begin
    if (Rst) begin
      r_Count <= 0;
      r_OutValid <= 0;
    end else begin
      r_Count <= r_next_Count;
      r_OutValid <= r_next_OutValid;
    end
  end

endmodule

