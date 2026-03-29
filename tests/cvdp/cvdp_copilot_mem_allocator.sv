module cvdp_leading_zero_cnt #(
  parameter DATA_WIDTH = 32,
  parameter REVERSE = 0
) (
  input [DATA_WIDTH-1:0] data,
  output [$clog2(DATA_WIDTH)-1:0] leading_zeros,
  output all_zeros
);

  localparam OUTW = $clog2(DATA_WIDTH);

  reg [OUTW-1:0] zcount;
  reg found;
  integer i;

  always @(*) begin
    zcount = 0;
    found = 1'b0;
    if (REVERSE == 1) begin
      // Trailing zero count: scan from LSB
      for (i = 0; i < DATA_WIDTH; i = i + 1) begin
        if (!found && data[i]) begin
          zcount = i[OUTW-1:0];
          found = 1'b1;
        end
      end
    end else begin
      // Leading zero count: scan from MSB
      for (i = 0; i < DATA_WIDTH; i = i + 1) begin
        if (!found && data[DATA_WIDTH - 1 - i]) begin
          zcount = i[OUTW-1:0];
          found = 1'b1;
        end
      end
    end
  end

  assign leading_zeros = zcount;
  assign all_zeros = (data == 0);

endmodule

module cvdp_copilot_mem_allocator #(
  parameter SIZE  = 4,
  parameter ADDRW = $clog2(SIZE)
) (
  input             clk,
  input             reset,
  input             acquire_en,
  output [ADDRW-1:0] acquire_addr,
  input             release_en,
  input [ADDRW-1:0] release_addr,
  output            empty,
  output            full
);

  reg [SIZE-1:0] free_slots, free_slots_n;
  reg [ADDRW-1:0] acquire_addr_r;
  reg empty_r, full_r;
  wire [ADDRW-1:0] free_index;
  wire full_d;

  cvdp_leading_zero_cnt #(
    .DATA_WIDTH(SIZE),
    .REVERSE(1)
  ) free_slots_sel (
    .data(free_slots_n),
    .leading_zeros(free_index),
    .all_zeros(full_d)
  );

  // Combinational next-state for free_slots
  always @(*) begin
    free_slots_n = free_slots;
    if (acquire_en) begin
      free_slots_n[acquire_addr_r] = 1'b0;
    end
    if (release_en) begin
      free_slots_n[release_addr] = 1'b1;
    end
  end

  // Sequential update
  always @(posedge clk) begin
    if (reset) begin
      free_slots    <= {SIZE{1'b1}};
      acquire_addr_r <= {ADDRW{1'b0}};
      empty_r       <= 1'b1;
      full_r        <= 1'b0;
    end else begin
      free_slots    <= free_slots_n;
      acquire_addr_r <= free_index;
      empty_r       <= &free_slots_n;
      full_r        <= full_d;
    end
  end

  assign acquire_addr = acquire_addr_r;
  assign empty        = empty_r;
  assign full         = full_r;

endmodule
