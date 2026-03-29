module cvdp_copilot_decode_firstbit #(
  parameter int InWidth_g = 32,
  parameter int InReg_g = 1,
  parameter int OutReg_g = 1,
  parameter int PlRegs_g = 1
) (
  input logic Clk,
  input logic Rst,
  input logic [InWidth_g-1:0] In_Data,
  input logic In_Valid,
  output logic [5-1:0] Out_FirstBit,
  output logic Out_Found,
  output logic Out_Valid
);

  logic [InWidth_g-1:0] in_data_r;
  logic in_valid_r;
  logic [5-1:0] pl_firstbit;
  logic pl_found;
  logic pl_valid;
  logic [5-1:0] out_firstbit_r;
  logic out_found_r;
  logic out_valid_r;
  logic [5-1:0] first_bit_comb;
  logic found_comb;
  logic [16-1:0] lo16;
  assign lo16 = in_data_r[15:0];
  logic [16-1:0] hi16;
  assign hi16 = in_data_r[31:16];
  logic lo_nz;
  assign lo_nz = lo16 != 0;
  logic [16-1:0] sel16;
  assign sel16 = lo_nz ? lo16 : hi16;
  logic [8-1:0] lo8;
  assign lo8 = sel16[7:0];
  logic [8-1:0] hi8;
  assign hi8 = sel16[15:8];
  logic lo8_nz;
  assign lo8_nz = lo8 != 0;
  logic [8-1:0] sel8;
  assign sel8 = lo8_nz ? lo8 : hi8;
  logic [4-1:0] lo4;
  assign lo4 = sel8[3:0];
  logic [4-1:0] hi4;
  assign hi4 = sel8[7:4];
  logic lo4_nz;
  assign lo4_nz = lo4 != 0;
  logic [4-1:0] sel4;
  assign sel4 = lo4_nz ? lo4 : hi4;
  logic [2-1:0] lo2;
  assign lo2 = sel4[1:0];
  logic [2-1:0] hi2;
  assign hi2 = sel4[3:2];
  logic lo2_nz;
  assign lo2_nz = lo2 != 0;
  logic [2-1:0] sel2;
  assign sel2 = lo2_nz ? lo2 : hi2;
  logic bit0;
  assign bit0 = sel2[0:0] != 0;
  assign found_comb = in_data_r != 0;
  assign first_bit_comb = 5'({~lo_nz, ~lo8_nz, ~lo4_nz, ~lo2_nz, ~bit0});
  always_ff @(posedge Clk or posedge Rst) begin
    if (Rst) begin
      in_data_r <= 0;
      in_valid_r <= 1'b0;
    end else begin
      in_data_r <= In_Data;
      in_valid_r <= In_Valid;
    end
  end
  always_ff @(posedge Clk or posedge Rst) begin
    if (Rst) begin
      pl_firstbit <= 0;
      pl_found <= 1'b0;
      pl_valid <= 1'b0;
    end else begin
      pl_firstbit <= first_bit_comb;
      pl_found <= found_comb;
      pl_valid <= in_valid_r;
    end
  end
  always_ff @(posedge Clk or posedge Rst) begin
    if (Rst) begin
      out_firstbit_r <= 0;
      out_found_r <= 1'b0;
      out_valid_r <= 1'b0;
    end else begin
      out_firstbit_r <= pl_firstbit;
      out_found_r <= pl_found;
      out_valid_r <= pl_valid;
    end
  end
  assign Out_FirstBit = out_firstbit_r;
  assign Out_Found = out_found_r;
  assign Out_Valid = out_valid_r;

endmodule

