// Ethernet MAC address learning table — textbook CAM use case.
//
// On the lookup side: given a destination MAC, return the egress port
// (or signal "miss" so the caller can broadcast). On the learn side:
// when a frame arrives, the caller picks a slot and writes
// (src_mac, ingress_port). v1 single-write port suffices because lookup
// and learn never write the CAM in the same cycle (lookup is purely
// combinational; only learn drives writes).
//
// Slot selection is the caller's job (round-robin in this demo). Real
// switches add aging / LRU on top.
module Mac_Cam #(
  parameter int DEPTH = 8,
  parameter int KEY_W = 48
) (
  input logic clk,
  input logic rst,
  input logic write_valid,
  input logic [2:0] write_idx,
  input logic [47:0] write_key,
  input logic write_set,
  input logic [47:0] search_key,
  output logic [7:0] search_mask,
  output logic search_any,
  output logic [2:0] search_first
);

  logic [DEPTH-1:0]      entry_valid_r;
  logic [KEY_W-1:0]      entry_key_r [DEPTH];
  
  always_comb begin
    for (int i = 0; i < DEPTH; i++) begin
      search_mask[i] = entry_valid_r[i] && (entry_key_r[i] == search_key);
    end
  end
  assign search_any = |search_mask;
  
  always_comb begin
    search_first = '0;
    for (int i = DEPTH-1; i >= 0; i--) begin
      if (search_mask[i]) search_first = i[$clog2(DEPTH)-1:0];
    end
  end
  
  always_ff @(posedge clk) begin
    if (rst) begin
      entry_valid_r <= '0;
    end else begin
      if (write_valid) begin
        if (write_set) begin
          entry_valid_r[write_idx] <= 1'b1;
          entry_key_r[write_idx] <= write_key;
        end else begin
          entry_valid_r[write_idx] <= 1'b0;
        end
      end
    end
  end
  
endmodule

// Ethernet MAC width
// $clog2(DEPTH)
// true=insert, false=clear (unused in demo)
module mac_table #(
  parameter int NUM_ENTRIES = 8,
  parameter int NUM_PORTS = 4,
  localparam int IDX_W = 3,
  localparam int PORT_W = 2
) (
  input logic clk,
  input logic rst,
  input logic [47:0] lookup_mac,
  output logic lookup_hit,
  output logic [PORT_W-1:0] lookup_port,
  input logic learn_valid,
  input logic [47:0] learn_mac,
  input logic [PORT_W-1:0] learn_port,
  input logic [IDX_W-1:0] learn_idx
);

  // Lookup interface (combinational)
  // miss → caller floods/broadcasts
  // Learn interface — writes a MAC→port binding into the slot
  // chosen by the caller (round-robin, LRU, etc.; out of scope here).
  // Per-entry port number — addressed by the cam's first-match index.
  // The CAM itself stores the MAC keys; this Vec stores the values.
  logic [NUM_ENTRIES-1:0] [PORT_W-1:0] port_table;
  logic [NUM_ENTRIES-1:0] cam_search_mask;
  // unused; required to bind cam port
  logic cam_search_any;
  logic [IDX_W-1:0] cam_search_first;
  Mac_Cam #(.DEPTH(NUM_ENTRIES), .KEY_W(48)) mac_cam (
    .clk(clk),
    .rst(rst),
    .write_valid(learn_valid),
    .write_idx(learn_idx),
    .write_key(learn_mac),
    .write_set(1'b1),
    .search_key(lookup_mac),
    .search_mask(cam_search_mask),
    .search_any(cam_search_any),
    .search_first(cam_search_first)
  );
  assign lookup_hit = cam_search_any;
  assign lookup_port = port_table[cam_search_first];
  always_ff @(posedge clk) begin
    if (rst) begin
      for (int __ri0 = 0; __ri0 < NUM_ENTRIES; __ri0++) begin
        port_table[__ri0] <= 0;
      end
    end else begin
      if (learn_valid) begin
        port_table[learn_idx] <= learn_port;
      end
    end
  end
  // synopsys translate_off
  // Auto-generated safety assertions (bounds / divide-by-zero)
  _auto_bound_vec_0: assert property (@(posedge clk) disable iff (rst) (learn_idx) < (NUM_ENTRIES))
    else $fatal(1, "BOUNDS VIOLATION: mac_table._auto_bound_vec_0");
  // synopsys translate_on

endmodule

