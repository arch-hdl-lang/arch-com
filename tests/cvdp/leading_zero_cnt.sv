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
  logic [DATA_WIDTH-1:0] bits;
  always_comb begin
    for (int i = 0; i <= DATA_WIDTH - 1; i++) begin
      bits[i] = data[i +: 1];
    end
  end
  logic found;
  assign found = bits[0] || bits[1] || bits[2] || bits[3] || bits[4] || bits[5] || bits[6] || bits[7] || bits[8] || bits[9] || bits[10] || bits[11] || bits[12] || bits[13] || bits[14] || bits[15] || bits[16] || bits[17] || bits[18] || bits[19] || bits[20] || bits[21] || bits[22] || bits[23] || bits[24] || bits[25] || bits[26] || bits[27] || bits[28] || bits[29] || bits[30] || bits[31];
  logic [4:0] index;
  assign index = (bits[0]) ? 5'd0 : (bits[1]) ? 5'd1 : (bits[2]) ? 5'd2 : (bits[3]) ? 5'd3 : (bits[4]) ? 5'd4 : (bits[5]) ? 5'd5 : (bits[6]) ? 5'd6 : (bits[7]) ? 5'd7 : (bits[8]) ? 5'd8 : (bits[9]) ? 5'd9 : (bits[10]) ? 5'd10 : (bits[11]) ? 5'd11 : (bits[12]) ? 5'd12 : (bits[13]) ? 5'd13 : (bits[14]) ? 5'd14 : (bits[15]) ? 5'd15 : (bits[16]) ? 5'd16 : (bits[17]) ? 5'd17 : (bits[18]) ? 5'd18 : (bits[19]) ? 5'd19 : (bits[20]) ? 5'd20 : (bits[21]) ? 5'd21 : (bits[22]) ? 5'd22 : (bits[23]) ? 5'd23 : (bits[24]) ? 5'd24 : (bits[25]) ? 5'd25 : (bits[26]) ? 5'd26 : (bits[27]) ? 5'd27 : (bits[28]) ? 5'd28 : (bits[29]) ? 5'd29 : (bits[30]) ? 5'd30 : (bits[31]) ? 5'd31 : 5'd0;
  assign leading_zeros = found ? index : OUT_WIDTH'($unsigned(0));
  assign all_zeros = data == 0;

endmodule

