// Ethernet MAC address learning table — refactored on cam v3 (value_type).
//
// The cam now stores (mac_addr, port) pairs directly via the value-payload
// bundle (VAL_W param + write_value/read_value ports). The previous version
// kept a parallel `port_table: Vec<UInt<PORT_W>, NUM_ENTRIES>` and indexed
// it by `cam.search_first` — pure boilerplate that v3 absorbs into the
// CAM itself.
module Mac_Cam #(
  parameter int DEPTH = 8,
  parameter int KEY_W = 48,
  parameter int VAL_W = 2
) (
  input logic clk,
  input logic rst,
  input logic write_valid,
  input logic [2:0] write_idx,
  input logic [47:0] write_key,
  input logic [1:0] write_value,
  input logic write_set,
  input logic [47:0] search_key,
  output logic [7:0] search_mask,
  output logic search_any,
  output logic [2:0] search_first,
  output logic [1:0] read_value
);

  logic [DEPTH-1:0]      entry_valid_r;
  logic [KEY_W-1:0]      entry_key_r [DEPTH];
  logic [VAL_W-1:0]      entry_value_r [DEPTH];
  
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
  
  assign read_value = entry_value_r[search_first];
  
  always_ff @(posedge clk) begin
    if (rst) begin
      entry_valid_r <= '0;
    end else begin
      if (write_valid) begin
        if (write_set) begin
          entry_valid_r[write_idx] <= 1'b1;
          entry_key_r[write_idx] <= write_key;
          entry_value_r[write_idx] <= write_value;
        end else begin
          entry_valid_r[write_idx] <= 1'b0;
        end
      end
    end
  end
  
endmodule

// Ethernet MAC width
// PORT_W
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
  // Learn interface — writes a (MAC → port) binding into the slot
  // chosen by the caller (round-robin, LRU, etc.; out of scope here).
  logic [NUM_ENTRIES-1:0] cam_search_mask;
  // unused; required to bind cam port
  logic cam_search_any;
  logic [IDX_W-1:0] cam_search_first;
  // unused; required to bind cam port
  logic [PORT_W-1:0] cam_read_value;
  Mac_Cam #(.DEPTH(NUM_ENTRIES), .KEY_W(48), .VAL_W(PORT_W)) mac_cam (
    .clk(clk),
    .rst(rst),
    .write_valid(learn_valid),
    .write_idx(learn_idx),
    .write_key(learn_mac),
    .write_value(learn_port),
    .write_set(1'b1),
    .search_key(lookup_mac),
    .search_mask(cam_search_mask),
    .search_any(cam_search_any),
    .search_first(cam_search_first),
    .read_value(cam_read_value)
  );
  assign lookup_hit = cam_search_any;
  assign lookup_port = cam_read_value;

endmodule

