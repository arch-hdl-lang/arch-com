package PkgL1d;
  typedef enum logic [0:0] {
    CMDLOAD = 1'd0,
    CMDSTORE = 1'd1
  } CacheCmd;
  
  typedef enum logic [3:0] {
    IDLE = 4'd0,
    LOOKUP = 4'd1,
    HITRESP = 4'd2,
    HITMERGE = 4'd3,
    HITWRITE = 4'd4,
    MISSALLOC = 4'd5,
    REFILL = 4'd6,
    REFILLWRITE = 4'd7,
    POSTREFILLSTORE = 4'd8,
    WRITEBACK = 4'd9
  } CacheState;
  
  typedef enum logic [1:0] {
    FILLIDLE = 2'd0,
    FILLSENDAR = 2'd1,
    FILLWAITR = 2'd2,
    FILLDONE = 2'd3
  } FillState;
  
  typedef enum logic [1:0] {
    WBIDLE = 2'd0,
    WBSENDAW = 2'd1,
    WBSENDW = 2'd2,
    WBWAITB = 2'd3
  } WbState;
  
endpackage

// Cache operation type
// Main cache controller states
// Full corrected state machine:
//
//  Idle ──► Lookup ──► HitResp ──────────────────────────────► Idle
//                   └─► HitMerge (store RMW: read old word)
//                         └─► HitWrite (write merged + mark dirty) ──► Idle
//                   └─► MissAlloc ──► Refill ──► RefillWrite
//                   │                              └─► PostRefillStore ──► Idle (store miss)
//                   │                              └─────────────────────► Idle (load miss)
//                   └─► Writeback ──► MissAlloc (see above)
//
// load hit: data SRAM result available this cycle → resp_valid
// store hit: old word available; compute merged = (old & ~mask) | (new & mask)
// store hit: write merged word + update dirty bit in tag
// write new tag (valid=1, dirty=0, new addr); start fill FSM
// wait for fill_done from AXI fill FSM
// write 8 fill words to data SRAM (beat counter 0..7)
// store-miss: apply store RMW on freshly filled line (like HitMerge+HitWrite)
// read 8 dirty words from data SRAM (beat counter 0..7); pass to wb FSM
// AXI4 fill FSM states
// AXI4 writeback FSM states
