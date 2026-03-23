// VerilogEval Prob141: 12-hour BCD clock
// domain SysDomain

module TopModule (
  input logic clk,
  input logic rst,
  input logic ena,
  output logic pm,
  output logic [8-1:0] hh,
  output logic [8-1:0] mm,
  output logic [8-1:0] ss
);

  logic [4-1:0] ss_lo;
  logic [4-1:0] ss_hi;
  logic [4-1:0] mm_lo;
  logic [4-1:0] mm_hi;
  logic [4-1:0] hh_lo;
  logic [4-1:0] hh_hi;
  logic pm_reg;
  assign ss[0] = ss_lo[0];
  assign ss[1] = ss_lo[1];
  assign ss[2] = ss_lo[2];
  assign ss[3] = ss_lo[3];
  assign ss[4] = ss_hi[0];
  assign ss[5] = ss_hi[1];
  assign ss[6] = ss_hi[2];
  assign ss[7] = ss_hi[3];
  assign mm[0] = mm_lo[0];
  assign mm[1] = mm_lo[1];
  assign mm[2] = mm_lo[2];
  assign mm[3] = mm_lo[3];
  assign mm[4] = mm_hi[0];
  assign mm[5] = mm_hi[1];
  assign mm[6] = mm_hi[2];
  assign mm[7] = mm_hi[3];
  assign hh[0] = hh_lo[0];
  assign hh[1] = hh_lo[1];
  assign hh[2] = hh_lo[2];
  assign hh[3] = hh_lo[3];
  assign hh[4] = hh_hi[0];
  assign hh[5] = hh_hi[1];
  assign hh[6] = hh_hi[2];
  assign hh[7] = hh_hi[3];
  assign pm = pm_reg;
  always_ff @(posedge clk) begin
    if (rst) begin
      hh_hi <= 1;
      hh_lo <= 2;
      mm_hi <= 0;
      mm_lo <= 0;
      pm_reg <= 0;
      ss_hi <= 0;
      ss_lo <= 0;
    end else begin
      if (ena) begin
        if ((ss_lo == 9)) begin
          ss_lo <= 0;
          if ((ss_hi == 5)) begin
            ss_hi <= 0;
            if ((mm_lo == 9)) begin
              mm_lo <= 0;
              if ((mm_hi == 5)) begin
                mm_hi <= 0;
                if (((hh_hi == 1) & (hh_lo == 2))) begin
                  hh_hi <= 0;
                  hh_lo <= 1;
                end else if (((hh_hi == 1) & (hh_lo == 1))) begin
                  hh_hi <= 1;
                  hh_lo <= 2;
                  pm_reg <= (~pm_reg);
                end else if ((hh_lo == 9)) begin
                  hh_lo <= 0;
                  hh_hi <= 4'((hh_hi + 1));
                end else begin
                  hh_lo <= 4'((hh_lo + 1));
                end
              end else begin
                mm_hi <= 4'((mm_hi + 1));
              end
            end else begin
              mm_lo <= 4'((mm_lo + 1));
            end
          end else begin
            ss_hi <= 4'((ss_hi + 1));
          end
        end else begin
          ss_lo <= 4'((ss_lo + 1));
        end
      end
    end
  end

endmodule

// Hour increment: 12->1, 11->12 (toggle pm), else increment
// 12:59:59 -> 01:00:00
// 11:59:59 -> 12:00:00, toggle PM
