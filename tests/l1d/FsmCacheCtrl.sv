// L1D cache controller FSM.
// States: Idle → Lookup → HitRdData → Idle           (load hit, 3 cycles)
//                       → WbCollect → WbWait → MissAlloc → Refill → RefillWrite → Idle (dirty evict)
//                       → MissAlloc → Refill → RefillWrite → Idle                       (clean miss)
//                                                          → PostRefillStore → Idle      (store miss)
module FsmCacheCtrl (
  input logic clk,
  input logic rst,
  input logic req_valid,
  output logic req_ready,
  input logic [64-1:0] req_vaddr,
  input logic [64-1:0] req_data,
  input logic [8-1:0] req_be,
  input logic req_is_store,
  output logic resp_valid,
  output logic [64-1:0] resp_data,
  output logic resp_error,
  output logic tag_rd_en [8-1:0],
  output logic [6-1:0] tag_rd_addr [8-1:0],
  input logic [54-1:0] tag_rd_data [8-1:0],
  output logic tag_wr_en [8-1:0],
  output logic [6-1:0] tag_wr_addr [8-1:0],
  output logic [54-1:0] tag_wr_data [8-1:0],
  output logic data_rd_en,
  output logic [12-1:0] data_rd_addr,
  input logic [64-1:0] data_rd_data,
  output logic data_wr_en,
  output logic [12-1:0] data_wr_addr,
  output logic [64-1:0] data_wr_data,
  output logic lru_rd_en,
  output logic [6-1:0] lru_rd_addr,
  input logic [7-1:0] lru_rd_data,
  output logic lru_wr_en,
  output logic [6-1:0] lru_wr_addr,
  output logic [7-1:0] lru_wr_data,
  output logic [7-1:0] lru_tree_in,
  output logic [3-1:0] lru_access_way,
  output logic lru_access_en,
  input logic [7-1:0] lru_tree_out,
  input logic [3-1:0] lru_victim_way,
  output logic fill_start,
  output logic [64-1:0] fill_addr,
  input logic fill_done,
  input logic [64-1:0] fill_word [8-1:0],
  output logic wb_start,
  output logic [64-1:0] wb_addr,
  input logic wb_done,
  output logic [64-1:0] wb_word [8-1:0]
);

  typedef enum logic [3:0] {
    IDLE = 4'd0,
    LOOKUP = 4'd1,
    HITRDDATA = 4'd2,
    MISSALLOC = 4'd3,
    REFILL = 4'd4,
    REFILLWRITE = 4'd5,
    POSTREFILLSTORE = 4'd6,
    WBCOLLECT = 4'd7,
    WBWAIT = 4'd8
  } FsmCacheCtrl_state_t;
  
  FsmCacheCtrl_state_t state_r, state_next;
  
  logic [64-1:0] req_addr_r;
  logic [64-1:0] req_data_r;
  logic [8-1:0] req_be_r;
  logic req_is_store_r;
  logic [3-1:0] hit_way_r;
  logic [3-1:0] victim_way_r;
  logic [52-1:0] victim_tag_r;
  logic [7-1:0] lru_tree_r;
  logic miss_is_store_r;
  logic [4-1:0] beat_ctr_r;
  logic [64-1:0] wb_buf [8-1:0];
  logic lookup_hit_r;
  logic lookup_victim_dirty_r;
  
  always_ff @(posedge clk) begin
    if (rst) begin
      state_r <= IDLE;
      beat_ctr_r <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        IDLE: begin
          // CPU request interface
          // CPU response interface
          // Tag SRAM — 8 ways (Vec ports, one element per way)
          // Data SRAM (addr = {set[5:0], way[2:0], word[2:0]} = 12 bits)
          // LRU SRAM
          // LRU update module (combinational)
          // Fill FSM
          // Writeback FSM
          // ── Latched request ───────────────────────────────────────────────────────
          // ── Lookup results ────────────────────────────────────────────────────────
          // ── Sub-state counter ─────────────────────────────────────────────────────
          // ── Writeback line buffer ─────────────────────────────────────────────────
          // ── Lookup decision registers (set in Lookup, read in transitions) ────────
          // ── Idle ──────────────────────────────────────────────────────────────────
          if (req_valid) begin
            req_addr_r <= req_vaddr;
            req_data_r <= req_data;
            req_be_r <= req_be;
            req_is_store_r <= req_is_store;
            beat_ctr_r <= 0;
          end
        end
        LOOKUP: begin
          // ── Lookup ────────────────────────────────────────────────────────────────
          // Tag/LRU data now available. Compute hit/miss, latch decision.
          // Issue data read for hit candidate (combinational, will be used in HitRdData)
          // Compute and latch hit way
          hit_way_r <= 0;
          if (tag_rd_data[1][53:2] == req_addr_r[63:12] & tag_rd_data[1][0]) begin
            hit_way_r <= 1;
          end else if (tag_rd_data[2][53:2] == req_addr_r[63:12] & tag_rd_data[2][0]) begin
            hit_way_r <= 2;
          end else if (tag_rd_data[3][53:2] == req_addr_r[63:12] & tag_rd_data[3][0]) begin
            hit_way_r <= 3;
          end else if (tag_rd_data[4][53:2] == req_addr_r[63:12] & tag_rd_data[4][0]) begin
            hit_way_r <= 4;
          end else if (tag_rd_data[5][53:2] == req_addr_r[63:12] & tag_rd_data[5][0]) begin
            hit_way_r <= 5;
          end else if (tag_rd_data[6][53:2] == req_addr_r[63:12] & tag_rd_data[6][0]) begin
            hit_way_r <= 6;
          end else if (tag_rd_data[7][53:2] == req_addr_r[63:12] & tag_rd_data[7][0]) begin
            hit_way_r <= 7;
          end
          lookup_hit_r <= tag_rd_data[0][53:2] == req_addr_r[63:12] & tag_rd_data[0][0] | tag_rd_data[1][53:2] == req_addr_r[63:12] & tag_rd_data[1][0] | tag_rd_data[2][53:2] == req_addr_r[63:12] & tag_rd_data[2][0] | tag_rd_data[3][53:2] == req_addr_r[63:12] & tag_rd_data[3][0] | tag_rd_data[4][53:2] == req_addr_r[63:12] & tag_rd_data[4][0] | tag_rd_data[5][53:2] == req_addr_r[63:12] & tag_rd_data[5][0] | tag_rd_data[6][53:2] == req_addr_r[63:12] & tag_rd_data[6][0] | tag_rd_data[7][53:2] == req_addr_r[63:12] & tag_rd_data[7][0];
          victim_way_r <= lru_victim_way;
          lru_tree_r <= lru_rd_data;
          miss_is_store_r <= req_is_store_r;
          // Capture victim tag and dirty bit via variable index
          victim_tag_r <= tag_rd_data[lru_victim_way][53:2];
          lookup_victim_dirty_r <= tag_rd_data[lru_victim_way][1];
        end
        REFILL: begin
          // ── HitRdData ─────────────────────────────────────────────────────────────
          // LRU update on hit
          // For store hit: write data + set dirty via variable-indexed tag write
          // ── MissAlloc ─────────────────────────────────────────────────────────────
          // Write new tag (valid=1, dirty=0) for victim way; issue fill; update LRU.
          // Write new tag: valid=1, dirty=0 → {tag, 01}
          // ── Refill ────────────────────────────────────────────────────────────────
          beat_ctr_r <= 0;
        end
        REFILLWRITE: begin
          // ── RefillWrite ───────────────────────────────────────────────────────────
          // Write 8 fill words to data SRAM (one per cycle, beat_ctr_r 0..7).
          // For load miss on last beat: return requested word directly
          beat_ctr_r <= 4'(beat_ctr_r + 1);
        end
        WBCOLLECT: begin
          // ── PostRefillStore ───────────────────────────────────────────────────────
          // After refill completes for a store miss: write req_data to victim way,
          // set dirty=1 in tag, drive response.
          // ── WbCollect ─────────────────────────────────────────────────────────────
          // Read dirty victim line (8 words, 9 cycles accounting for SRAM latency 1).
          beat_ctr_r <= 4'(beat_ctr_r + 1);
          if (beat_ctr_r == 1) begin
            wb_buf[0] <= data_rd_data;
          end else if (beat_ctr_r == 2) begin
            wb_buf[1] <= data_rd_data;
          end else if (beat_ctr_r == 3) begin
            wb_buf[2] <= data_rd_data;
          end else if (beat_ctr_r == 4) begin
            wb_buf[3] <= data_rd_data;
          end else if (beat_ctr_r == 5) begin
            wb_buf[4] <= data_rd_data;
          end else if (beat_ctr_r == 6) begin
            wb_buf[5] <= data_rd_data;
          end else if (beat_ctr_r == 7) begin
            wb_buf[6] <= data_rd_data;
          end else if (beat_ctr_r == 8) begin
            wb_buf[7] <= data_rd_data;
          end
        end
        WBWAIT: begin
          // ── WbWait ────────────────────────────────────────────────────────────────
          // Drive wb_start and wb line data; wait for writeback FSM to complete.
          beat_ctr_r <= 0;
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      IDLE: begin
        if (req_valid) state_next = LOOKUP;
      end
      LOOKUP: begin
        state_next = HITRDDATA;
      end
      HITRDDATA: begin
        if (lookup_hit_r) state_next = IDLE;
        else if (~lookup_hit_r & lookup_victim_dirty_r) state_next = WBCOLLECT;
        else if (~lookup_hit_r & ~lookup_victim_dirty_r) state_next = MISSALLOC;
      end
      MISSALLOC: begin
        state_next = REFILL;
      end
      REFILL: begin
        if (fill_done) state_next = REFILLWRITE;
      end
      REFILLWRITE: begin
        if (beat_ctr_r == 7 & ~miss_is_store_r) state_next = IDLE;
        else if (beat_ctr_r == 7 & miss_is_store_r) state_next = POSTREFILLSTORE;
      end
      POSTREFILLSTORE: begin
        state_next = IDLE;
      end
      WBCOLLECT: begin
        if (beat_ctr_r == 8) state_next = WBWAIT;
      end
      WBWAIT: begin
        if (wb_done) state_next = MISSALLOC;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    req_ready = 1'b0;
    resp_valid = 1'b0;
    resp_data = 0;
    resp_error = 1'b0;
    for (int i = 0; i <= 7; i++) begin
      tag_rd_en[i] = 1'b0;
      tag_rd_addr[i] = 0;
      tag_wr_en[i] = 1'b0;
      tag_wr_addr[i] = 0;
      tag_wr_data[i] = 0;
      wb_word[i] = 0;
    end
    data_rd_en = 1'b0;
    data_rd_addr = 0;
    data_wr_en = 1'b0;
    data_wr_addr = 0;
    data_wr_data = 0;
    lru_rd_en = 1'b0;
    lru_rd_addr = 0;
    lru_wr_en = 1'b0;
    lru_wr_addr = 0;
    lru_wr_data = 0;
    lru_tree_in = 0;
    lru_access_way = 0;
    lru_access_en = 1'b0;
    fill_start = 1'b0;
    fill_addr = 0;
    wb_start = 1'b0;
    wb_addr = 0;
    case (state_r)
      IDLE: begin
        req_ready = 1'b1;
        if (req_valid) begin
          for (int i = 0; i <= 7; i++) begin
            tag_rd_en[i] = 1'b1;
            tag_rd_addr[i] = req_vaddr[11:6];
          end
          lru_rd_en = 1'b1;
          lru_rd_addr = req_vaddr[11:6];
        end
      end
      LOOKUP: begin
        lru_tree_in = lru_rd_data;
        lru_access_way = 0;
        lru_access_en = 1'b0;
        data_rd_en = 1'b1;
        data_rd_addr = 0;
        if (tag_rd_data[0][53:2] == req_addr_r[63:12] & tag_rd_data[0][0]) begin
          data_rd_addr = {req_addr_r[11:6], 3'(0), req_addr_r[5:3]};
        end else if (tag_rd_data[1][53:2] == req_addr_r[63:12] & tag_rd_data[1][0]) begin
          data_rd_addr = {req_addr_r[11:6], 3'(1), req_addr_r[5:3]};
        end else if (tag_rd_data[2][53:2] == req_addr_r[63:12] & tag_rd_data[2][0]) begin
          data_rd_addr = {req_addr_r[11:6], 3'(2), req_addr_r[5:3]};
        end else if (tag_rd_data[3][53:2] == req_addr_r[63:12] & tag_rd_data[3][0]) begin
          data_rd_addr = {req_addr_r[11:6], 3'(3), req_addr_r[5:3]};
        end else if (tag_rd_data[4][53:2] == req_addr_r[63:12] & tag_rd_data[4][0]) begin
          data_rd_addr = {req_addr_r[11:6], 3'(4), req_addr_r[5:3]};
        end else if (tag_rd_data[5][53:2] == req_addr_r[63:12] & tag_rd_data[5][0]) begin
          data_rd_addr = {req_addr_r[11:6], 3'(5), req_addr_r[5:3]};
        end else if (tag_rd_data[6][53:2] == req_addr_r[63:12] & tag_rd_data[6][0]) begin
          data_rd_addr = {req_addr_r[11:6], 3'(6), req_addr_r[5:3]};
        end else if (tag_rd_data[7][53:2] == req_addr_r[63:12] & tag_rd_data[7][0]) begin
          data_rd_addr = {req_addr_r[11:6], 3'(7), req_addr_r[5:3]};
        end else begin
          data_rd_en = 1'b0;
        end
      end
      HITRDDATA: begin
        resp_valid = lookup_hit_r;
        resp_data = data_rd_data;
        if (lookup_hit_r) begin
          lru_tree_in = lru_tree_r;
          lru_access_way = hit_way_r;
          lru_access_en = 1'b1;
          lru_wr_en = 1'b1;
          lru_wr_addr = req_addr_r[11:6];
          lru_wr_data = lru_tree_out;
          if (req_is_store_r) begin
            data_wr_en = 1'b1;
            data_wr_addr = {req_addr_r[11:6], hit_way_r, req_addr_r[5:3]};
            data_wr_data = req_data_r;
            tag_wr_en[hit_way_r] = 1'b1;
            tag_wr_addr[hit_way_r] = req_addr_r[11:6];
            tag_wr_data[hit_way_r] = {req_addr_r[63:12], 2'(3)};
          end
        end
      end
      MISSALLOC: begin
        fill_start = 1'b1;
        fill_addr = req_addr_r;
        lru_tree_in = lru_tree_r;
        lru_access_way = victim_way_r;
        lru_access_en = 1'b1;
        lru_wr_en = 1'b1;
        lru_wr_addr = req_addr_r[11:6];
        lru_wr_data = lru_tree_out;
        tag_wr_en[victim_way_r] = 1'b1;
        tag_wr_addr[victim_way_r] = req_addr_r[11:6];
        tag_wr_data[victim_way_r] = {req_addr_r[63:12], 2'(1)};
      end
      REFILL: begin
      end
      REFILLWRITE: begin
        data_wr_en = 1'b1;
        data_wr_data = fill_word[0];
        data_wr_addr = {req_addr_r[11:6], victim_way_r, 3'(0)};
        if (beat_ctr_r == 0) begin
          data_wr_addr = {req_addr_r[11:6], victim_way_r, 3'(0)};
          data_wr_data = fill_word[0];
        end else if (beat_ctr_r == 1) begin
          data_wr_addr = {req_addr_r[11:6], victim_way_r, 3'(1)};
          data_wr_data = fill_word[1];
        end else if (beat_ctr_r == 2) begin
          data_wr_addr = {req_addr_r[11:6], victim_way_r, 3'(2)};
          data_wr_data = fill_word[2];
        end else if (beat_ctr_r == 3) begin
          data_wr_addr = {req_addr_r[11:6], victim_way_r, 3'(3)};
          data_wr_data = fill_word[3];
        end else if (beat_ctr_r == 4) begin
          data_wr_addr = {req_addr_r[11:6], victim_way_r, 3'(4)};
          data_wr_data = fill_word[4];
        end else if (beat_ctr_r == 5) begin
          data_wr_addr = {req_addr_r[11:6], victim_way_r, 3'(5)};
          data_wr_data = fill_word[5];
        end else if (beat_ctr_r == 6) begin
          data_wr_addr = {req_addr_r[11:6], victim_way_r, 3'(6)};
          data_wr_data = fill_word[6];
        end else if (beat_ctr_r == 7) begin
          data_wr_addr = {req_addr_r[11:6], victim_way_r, 3'(7)};
          data_wr_data = fill_word[7];
        end
        if (beat_ctr_r == 7 & ~miss_is_store_r) begin
          resp_valid = 1'b1;
          resp_data = fill_word[req_addr_r[5:3]];
        end
      end
      POSTREFILLSTORE: begin
        data_wr_en = 1'b1;
        data_wr_addr = {req_addr_r[11:6], victim_way_r, req_addr_r[5:3]};
        data_wr_data = req_data_r;
        resp_valid = 1'b1;
        resp_data = req_data_r;
        tag_wr_en[victim_way_r] = 1'b1;
        tag_wr_addr[victim_way_r] = req_addr_r[11:6];
        tag_wr_data[victim_way_r] = {req_addr_r[63:12], 2'(3)};
      end
      WBCOLLECT: begin
        if (beat_ctr_r == 0) begin
          data_rd_en = 1'b1;
          data_rd_addr = {req_addr_r[11:6], victim_way_r, 3'(0)};
        end else if (beat_ctr_r == 1) begin
          data_rd_en = 1'b1;
          data_rd_addr = {req_addr_r[11:6], victim_way_r, 3'(1)};
        end else if (beat_ctr_r == 2) begin
          data_rd_en = 1'b1;
          data_rd_addr = {req_addr_r[11:6], victim_way_r, 3'(2)};
        end else if (beat_ctr_r == 3) begin
          data_rd_en = 1'b1;
          data_rd_addr = {req_addr_r[11:6], victim_way_r, 3'(3)};
        end else if (beat_ctr_r == 4) begin
          data_rd_en = 1'b1;
          data_rd_addr = {req_addr_r[11:6], victim_way_r, 3'(4)};
        end else if (beat_ctr_r == 5) begin
          data_rd_en = 1'b1;
          data_rd_addr = {req_addr_r[11:6], victim_way_r, 3'(5)};
        end else if (beat_ctr_r == 6) begin
          data_rd_en = 1'b1;
          data_rd_addr = {req_addr_r[11:6], victim_way_r, 3'(6)};
        end else if (beat_ctr_r == 7) begin
          data_rd_en = 1'b1;
          data_rd_addr = {req_addr_r[11:6], victim_way_r, 3'(7)};
        end
      end
      WBWAIT: begin
        wb_start = 1'b1;
        wb_addr = 64'($unsigned(victim_tag_r)) << 12 | 64'($unsigned(req_addr_r[11:6])) << 6;
        for (int i = 0; i <= 7; i++) begin
          wb_word[i] = wb_buf[i];
        end
      end
      default: ;
    endcase
  end

endmodule

