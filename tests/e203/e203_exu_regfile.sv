// E203 HBirdv2 integer register file
// 32 x 32-bit, 2 async read ports, 1 sync write port.
// x0 hardwired to 0. x1 exposed as output for link register.
// No reset on data entries (matches E203 spec).
module e203_exu_regfile #(
  parameter int XLEN = 32,
  parameter int NREGS = 32
) (
  input logic clk,
  input logic rst_n,
  input logic test_mode,
  input logic [5-1:0] read_src1_idx,
  output logic [32-1:0] read_src1_dat,
  input logic [5-1:0] read_src2_idx,
  output logic [32-1:0] read_src2_dat,
  input logic wbck_dest_wen,
  input logic [5-1:0] wbck_dest_idx,
  input logic [32-1:0] wbck_dest_dat,
  output logic [32-1:0] x1_r
);

  // Read port 1
  // Read port 2
  // Write port
  // x1 (ra) exposed for IFU link-register read
  // Register file storage — 32 registers, no reset
  logic [32-1:0] rf_0 = 0;
  logic [32-1:0] rf_1 = 0;
  logic [32-1:0] rf_2 = 0;
  logic [32-1:0] rf_3 = 0;
  logic [32-1:0] rf_4 = 0;
  logic [32-1:0] rf_5 = 0;
  logic [32-1:0] rf_6 = 0;
  logic [32-1:0] rf_7 = 0;
  logic [32-1:0] rf_8 = 0;
  logic [32-1:0] rf_9 = 0;
  logic [32-1:0] rf_10 = 0;
  logic [32-1:0] rf_11 = 0;
  logic [32-1:0] rf_12 = 0;
  logic [32-1:0] rf_13 = 0;
  logic [32-1:0] rf_14 = 0;
  logic [32-1:0] rf_15 = 0;
  logic [32-1:0] rf_16 = 0;
  logic [32-1:0] rf_17 = 0;
  logic [32-1:0] rf_18 = 0;
  logic [32-1:0] rf_19 = 0;
  logic [32-1:0] rf_20 = 0;
  logic [32-1:0] rf_21 = 0;
  logic [32-1:0] rf_22 = 0;
  logic [32-1:0] rf_23 = 0;
  logic [32-1:0] rf_24 = 0;
  logic [32-1:0] rf_25 = 0;
  logic [32-1:0] rf_26 = 0;
  logic [32-1:0] rf_27 = 0;
  logic [32-1:0] rf_28 = 0;
  logic [32-1:0] rf_29 = 0;
  logic [32-1:0] rf_30 = 0;
  logic [32-1:0] rf_31 = 0;
  // Write port — x0 is hardwired to 0 (skip writes to index 0)
  always_ff @(posedge clk) begin
    if (wbck_dest_wen & wbck_dest_idx != 0) begin
      if (wbck_dest_idx == 1) begin
        rf_1 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 2) begin
        rf_2 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 3) begin
        rf_3 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 4) begin
        rf_4 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 5) begin
        rf_5 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 6) begin
        rf_6 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 7) begin
        rf_7 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 8) begin
        rf_8 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 9) begin
        rf_9 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 10) begin
        rf_10 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 11) begin
        rf_11 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 12) begin
        rf_12 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 13) begin
        rf_13 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 14) begin
        rf_14 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 15) begin
        rf_15 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 16) begin
        rf_16 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 17) begin
        rf_17 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 18) begin
        rf_18 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 19) begin
        rf_19 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 20) begin
        rf_20 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 21) begin
        rf_21 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 22) begin
        rf_22 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 23) begin
        rf_23 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 24) begin
        rf_24 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 25) begin
        rf_25 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 26) begin
        rf_26 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 27) begin
        rf_27 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 28) begin
        rf_28 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 29) begin
        rf_29 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 30) begin
        rf_30 <= wbck_dest_dat;
      end else if (wbck_dest_idx == 31) begin
        rf_31 <= wbck_dest_dat;
      end
    end
  end
  // Read ports — async (combinational), x0 always reads 0
  always_comb begin
    if (read_src1_idx == 0) begin
      read_src1_dat = 0;
    end else if (read_src1_idx == 1) begin
      read_src1_dat = rf_1;
    end else if (read_src1_idx == 2) begin
      read_src1_dat = rf_2;
    end else if (read_src1_idx == 3) begin
      read_src1_dat = rf_3;
    end else if (read_src1_idx == 4) begin
      read_src1_dat = rf_4;
    end else if (read_src1_idx == 5) begin
      read_src1_dat = rf_5;
    end else if (read_src1_idx == 6) begin
      read_src1_dat = rf_6;
    end else if (read_src1_idx == 7) begin
      read_src1_dat = rf_7;
    end else if (read_src1_idx == 8) begin
      read_src1_dat = rf_8;
    end else if (read_src1_idx == 9) begin
      read_src1_dat = rf_9;
    end else if (read_src1_idx == 10) begin
      read_src1_dat = rf_10;
    end else if (read_src1_idx == 11) begin
      read_src1_dat = rf_11;
    end else if (read_src1_idx == 12) begin
      read_src1_dat = rf_12;
    end else if (read_src1_idx == 13) begin
      read_src1_dat = rf_13;
    end else if (read_src1_idx == 14) begin
      read_src1_dat = rf_14;
    end else if (read_src1_idx == 15) begin
      read_src1_dat = rf_15;
    end else if (read_src1_idx == 16) begin
      read_src1_dat = rf_16;
    end else if (read_src1_idx == 17) begin
      read_src1_dat = rf_17;
    end else if (read_src1_idx == 18) begin
      read_src1_dat = rf_18;
    end else if (read_src1_idx == 19) begin
      read_src1_dat = rf_19;
    end else if (read_src1_idx == 20) begin
      read_src1_dat = rf_20;
    end else if (read_src1_idx == 21) begin
      read_src1_dat = rf_21;
    end else if (read_src1_idx == 22) begin
      read_src1_dat = rf_22;
    end else if (read_src1_idx == 23) begin
      read_src1_dat = rf_23;
    end else if (read_src1_idx == 24) begin
      read_src1_dat = rf_24;
    end else if (read_src1_idx == 25) begin
      read_src1_dat = rf_25;
    end else if (read_src1_idx == 26) begin
      read_src1_dat = rf_26;
    end else if (read_src1_idx == 27) begin
      read_src1_dat = rf_27;
    end else if (read_src1_idx == 28) begin
      read_src1_dat = rf_28;
    end else if (read_src1_idx == 29) begin
      read_src1_dat = rf_29;
    end else if (read_src1_idx == 30) begin
      read_src1_dat = rf_30;
    end else begin
      read_src1_dat = rf_31;
    end
    if (read_src2_idx == 0) begin
      read_src2_dat = 0;
    end else if (read_src2_idx == 1) begin
      read_src2_dat = rf_1;
    end else if (read_src2_idx == 2) begin
      read_src2_dat = rf_2;
    end else if (read_src2_idx == 3) begin
      read_src2_dat = rf_3;
    end else if (read_src2_idx == 4) begin
      read_src2_dat = rf_4;
    end else if (read_src2_idx == 5) begin
      read_src2_dat = rf_5;
    end else if (read_src2_idx == 6) begin
      read_src2_dat = rf_6;
    end else if (read_src2_idx == 7) begin
      read_src2_dat = rf_7;
    end else if (read_src2_idx == 8) begin
      read_src2_dat = rf_8;
    end else if (read_src2_idx == 9) begin
      read_src2_dat = rf_9;
    end else if (read_src2_idx == 10) begin
      read_src2_dat = rf_10;
    end else if (read_src2_idx == 11) begin
      read_src2_dat = rf_11;
    end else if (read_src2_idx == 12) begin
      read_src2_dat = rf_12;
    end else if (read_src2_idx == 13) begin
      read_src2_dat = rf_13;
    end else if (read_src2_idx == 14) begin
      read_src2_dat = rf_14;
    end else if (read_src2_idx == 15) begin
      read_src2_dat = rf_15;
    end else if (read_src2_idx == 16) begin
      read_src2_dat = rf_16;
    end else if (read_src2_idx == 17) begin
      read_src2_dat = rf_17;
    end else if (read_src2_idx == 18) begin
      read_src2_dat = rf_18;
    end else if (read_src2_idx == 19) begin
      read_src2_dat = rf_19;
    end else if (read_src2_idx == 20) begin
      read_src2_dat = rf_20;
    end else if (read_src2_idx == 21) begin
      read_src2_dat = rf_21;
    end else if (read_src2_idx == 22) begin
      read_src2_dat = rf_22;
    end else if (read_src2_idx == 23) begin
      read_src2_dat = rf_23;
    end else if (read_src2_idx == 24) begin
      read_src2_dat = rf_24;
    end else if (read_src2_idx == 25) begin
      read_src2_dat = rf_25;
    end else if (read_src2_idx == 26) begin
      read_src2_dat = rf_26;
    end else if (read_src2_idx == 27) begin
      read_src2_dat = rf_27;
    end else if (read_src2_idx == 28) begin
      read_src2_dat = rf_28;
    end else if (read_src2_idx == 29) begin
      read_src2_dat = rf_29;
    end else if (read_src2_idx == 30) begin
      read_src2_dat = rf_30;
    end else begin
      read_src2_dat = rf_31;
    end
    // x1 (ra) direct output
    x1_r = rf_1;
  end

endmodule

