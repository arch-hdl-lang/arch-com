module lfu_counter_policy #(
  parameter int NWAYS = 4,
  parameter int NINDEXES = 32,
  parameter int COUNTERW = 2,
  parameter int MAX_FREQUENCY = (1 << COUNTERW) - 1
) (
  input logic clock,
  input logic reset,
  input logic [$clog2(NINDEXES)-1:0] index,
  input logic [$clog2(NWAYS)-1:0] way_select,
  input logic access,
  input logic hit,
  output logic [$clog2(NWAYS)-1:0] way_replace
);

  // Frequency array: each index has NWAYS counters, each COUNTERW bits
  logic [NWAYS * COUNTERW-1:0] frequency [NINDEXES-1:0];
  // Combinational signals
  logic [COUNTERW-1:0] min_val;
  logic [$clog2(NWAYS)-1:0] lfu_way;
  logic [NWAYS * COUNTERW-1:0] cur_freq;
  logic [COUNTERW-1:0] accessed_cnt;
  logic [$clog2(NINDEXES)-1:0] idx;
  assign idx = index;
  logic [$clog2(NWAYS)-1:0] ws;
  assign ws = way_select;
  logic [COUNTERW-1:0] max_f;
  assign max_f = MAX_FREQUENCY[COUNTERW - 1:0];
  // Combinational: extract counters for current index and find LFU way
  always_comb begin
    cur_freq = frequency[idx];
    // Extract accessed way's counter
    accessed_cnt = cur_freq[ws * COUNTERW +: COUNTERW];
    // Find minimum counter value (LFU), lower index wins ties
    min_val = cur_freq[COUNTERW - 1:0];
    lfu_way = 0;
    for (int i = 1; i <= NWAYS - 1; i++) begin
      if (cur_freq[i * COUNTERW +: COUNTERW] < min_val) begin
        min_val = cur_freq[i * COUNTERW +: COUNTERW];
        lfu_way = $clog2(NWAYS)'(i);
      end
    end
    way_replace = lfu_way;
  end
  // Sequential logic for frequency counter updates
  always_ff @(posedge clock or posedge reset) begin
    if (reset) begin
      for (int __ri0 = 0; __ri0 < NINDEXES; __ri0++) begin
        frequency[__ri0] <= 0;
      end
    end else begin
      if (access) begin
        if (hit) begin
          // Cache hit
          if (accessed_cnt < max_f) begin
            // Increment the accessed way's counter
            frequency[idx][ws * COUNTERW +: COUNTERW] <= COUNTERW'(accessed_cnt + 1);
          end else begin
            // Accessed way already at MAX_FREQUENCY: decrement others > 2
            for (int i = 0; i <= NWAYS - 1; i++) begin
              if ($clog2(NWAYS)'(i) != ws) begin
                if (cur_freq[i * COUNTERW +: COUNTERW] > 2) begin
                  frequency[idx][i * COUNTERW +: COUNTERW] <= COUNTERW'(cur_freq[i * COUNTERW +: COUNTERW] - 1);
                end
              end
            end
          end
        end else begin
          // Cache miss: set replacement way counter to 1
          frequency[idx][lfu_way * COUNTERW +: COUNTERW] <= 1;
        end
      end
    end
  end

endmodule

