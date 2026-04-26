// Auto-generated result struct(s) for Vec.find_first
typedef struct packed { logic found; logic [4:0] index; } __ArchFindResult_5;

module leading_zero_cnt #(
  parameter int DATA_WIDTH = 32,
  parameter int REVERSE = 0,
  parameter int OUT_WIDTH = $clog2(DATA_WIDTH)
) (
  input logic [DATA_WIDTH-1:0] data,
  output logic [OUT_WIDTH-1:0] leading_zeros,
  output logic all_zeros
);

  // View `data`'s bits as a Vec so the priority-encoder pattern
  // collapses into a single `find_first` call.
  logic found;
  assign found = data[0] || data[1] || data[2] || data[3] || data[4] || data[5] || data[6] || data[7] || data[8] || data[9] || data[10] || data[11] || data[12] || data[13] || data[14] || data[15] || data[16] || data[17] || data[18] || data[19] || data[20] || data[21] || data[22] || data[23] || data[24] || data[25] || data[26] || data[27] || data[28] || data[29] || data[30] || data[31];
  logic [4:0] index;
  assign index = (data[0]) ? 5'd0 : (data[1]) ? 5'd1 : (data[2]) ? 5'd2 : (data[3]) ? 5'd3 : (data[4]) ? 5'd4 : (data[5]) ? 5'd5 : (data[6]) ? 5'd6 : (data[7]) ? 5'd7 : (data[8]) ? 5'd8 : (data[9]) ? 5'd9 : (data[10]) ? 5'd10 : (data[11]) ? 5'd11 : (data[12]) ? 5'd12 : (data[13]) ? 5'd13 : (data[14]) ? 5'd14 : (data[15]) ? 5'd15 : (data[16]) ? 5'd16 : (data[17]) ? 5'd17 : (data[18]) ? 5'd18 : (data[19]) ? 5'd19 : (data[20]) ? 5'd20 : (data[21]) ? 5'd21 : (data[22]) ? 5'd22 : (data[23]) ? 5'd23 : (data[24]) ? 5'd24 : (data[25]) ? 5'd25 : (data[26]) ? 5'd26 : (data[27]) ? 5'd27 : (data[28]) ? 5'd28 : (data[29]) ? 5'd29 : (data[30]) ? 5'd30 : (data[31]) ? 5'd31 : 5'd0;
  assign leading_zeros = found ? index : OUT_WIDTH'($unsigned(0));
  assign all_zeros = data == 0;

endmodule

