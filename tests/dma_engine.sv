// =============================================================================
// Simple DMA Engine
// Demonstrates: domain, struct, enum, regfile, ram, fifo, counter,
//               arbiter, fsm, module (comb + reg blocks, match, inst)
// =============================================================================
// ── Clock domain ─────────────────────────────────────────────────────────────
// domain SysDomain
//   freq_mhz: 100

// ── Shared types ──────────────────────────────────────────────────────────────
// DmaDescriptor: describes a DMA transfer; DescTable stores words in this layout.
// BusCmd:        would drive the MemArbiter request encoding in a fuller design.
typedef struct packed { // fields: LSB→MSB (reverse of declaration order)
  logic [8-1:0] flags;
  logic [16-1:0] length;
  logic [32-1:0] dst_addr;
  logic [32-1:0] src_addr;
} DmaDescriptor;

typedef enum logic [1:0] {
  BUSIDLE = 2'd0,
  BUSREAD = 2'd1,
  BUSWRITE = 2'd2
} BusCmd;

// ── CSR register file (src_addr, dst_addr, length, control) ──────────────────
// Instantiated inside DmaEngine as `inst regs: DmaRegs`.
// Two read ports: port 0 used for APB read-back; port 1 tied off.
module DmaRegs #(
  parameter int NREGS = 8,
  parameter int DATA_WIDTH = 32
) (
  input logic clk,
  input logic rst,
  input logic [3-1:0] read0_addr,
  output logic [32-1:0] read0_data,
  input logic [3-1:0] read1_addr,
  output logic [32-1:0] read1_data,
  input logic write_en,
  input logic [3-1:0] write_addr,
  input logic [32-1:0] write_data
);

  logic [DATA_WIDTH-1:0] rf_data [0:NREGS-1];
  
  always_ff @(posedge clk) begin
    if (rst) begin
      rf_data[0] <= 0;
      rf_data[1] <= 0;
      rf_data[2] <= 0;
      rf_data[3] <= 0;
    end else begin
      if (write_en)
        rf_data[write_addr] <= write_data;
    end
  end
  
  always_comb begin
    if (write_en && write_addr == read0_addr)
      read0_data = write_data;
    else
      read0_data = rf_data[read0_addr];
    if (write_en && write_addr == read1_addr)
      read1_data = write_data;
    else
      read1_data = rf_data[read1_addr];
  end

endmodule

// ── Descriptor table RAM (simple_dual: CPU writes, DMA reads) ─────────────────
// Instantiated inside DmaEngine as `inst desc: DescTable`.
// APB writes program descriptors; DMA fetches src/dst via the read port on start.
module DescTable #(
  parameter int DEPTH = 16,
  parameter int DATA_WIDTH = 32
) (
  input logic clk,
  input logic [4-1:0] rd_port_addr,
  input logic rd_port_en,
  output logic [DATA_WIDTH-1:0] rd_port_rdata,
  input logic [4-1:0] wr_port_addr,
  input logic wr_port_en,
  input logic wr_port_wen,
  input logic [DATA_WIDTH-1:0] wr_port_wdata
);

  logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];
  logic [DATA_WIDTH-1:0] rd_port_rdata_r;
  
  always_ff @(posedge clk) begin
    if (wr_port_en)
      mem[wr_port_addr] <= wr_port_wdata;
    if (rd_port_en)
      rd_port_rdata_r <= mem[rd_port_addr];
  end
  assign rd_port_rdata = rd_port_rdata_r;
  
  initial begin
    for (int i = 0; i < DEPTH; i++) mem[i] = '0;
  end

endmodule

// ── Write-side data FIFO (decouples read and write phases) ────────────────────
// Instantiated inside DmaEngine as `inst wb: WriteBuffer`.
// Reads push into the FIFO; writes pop out one cycle later, allowing write
// backpressure to stall writes without stalling the read master.
module WriteBuffer #(
  parameter int DEPTH = 8,
  parameter int DATA_WIDTH = 32
) (
  input logic clk,
  input logic rst,
  input logic push_valid,
  output logic push_ready,
  input logic [DATA_WIDTH-1:0] push_data,
  output logic pop_valid,
  input logic pop_ready,
  output logic [DATA_WIDTH-1:0] pop_data
);

  localparam int PTR_W = $clog2(DEPTH) + 1;
  
  logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];
  logic [PTR_W-1:0]     wr_ptr;
  logic [PTR_W-1:0]     rd_ptr;
  logic                 full;
  logic                 empty;
  
  // Full when MSBs differ and lower bits match
  assign full        = (wr_ptr[PTR_W-1] != rd_ptr[PTR_W-1]) &&
                       (wr_ptr[PTR_W-2:0] == rd_ptr[PTR_W-2:0]);
  assign empty       = (wr_ptr == rd_ptr);
  assign push_ready  = !full;
  assign pop_valid   = !empty;
  assign pop_data    = mem[rd_ptr[PTR_W-2:0]];
  
  always_ff @(posedge clk) begin
    if (rst) begin
      wr_ptr <= '0;
      rd_ptr <= '0;
    end else begin
      if (push_valid && push_ready) begin
        mem[wr_ptr[PTR_W-2:0]] <= push_data;
        wr_ptr <= wr_ptr + 1;
      end
      if (pop_valid && pop_ready) begin
        rd_ptr <= rd_ptr + 1;
      end
    end
  end

endmodule

// ── Beat counter (counts transferred words, wraps at MAX) ─────────────────────
// Instantiated inside DmaEngine as `inst beat: BeatCounter`.
module BeatCounter #(
  parameter int MAX = 255
) (
  input logic clk,
  input logic rst,
  input logic clear,
  input logic inc,
  output logic [8-1:0] value,
  output logic at_max
);

  logic [8-1:0] count_r;
  always_ff @(posedge clk) begin
    if (rst) count_r <= 0;
    else if (clear) count_r <= 0;
    else if (inc) begin
      if (count_r == 8'(MAX)) count_r <= 0;
      else count_r <= count_r + 1;
    end
  end
  assign value = count_r;
  assign at_max = (count_r == 8'(MAX));

endmodule

// ── Memory bus arbiter (round-robin between DMA read and write masters) ───────
// Would arbitrate shared memory access when read and write phases overlap.
module MemArbiter #(
  parameter int NUM_REQ = 2
) (
  input logic clk,
  input logic rst,
  output logic grant_valid,
  output logic [1-1:0] grant_requester,
  input logic [NUM_REQ-1:0] request_valid,
  output logic [NUM_REQ-1:0] request_ready
);

  logic [1-1:0] rr_ptr_r;
  integer arb_i;
  logic arb_found;
  
  always_ff @(posedge clk) begin
    if (rst) rr_ptr_r <= '0;
    else if (grant_valid) rr_ptr_r <= rr_ptr_r + 1;
  end
  
  always_comb begin
    grant_valid = 1'b0;
    request_ready = '0;
    grant_requester = '0;
    arb_found = 1'b0;
    for (arb_i = 0; arb_i < 2; arb_i++) begin
      if (!arb_found && request_valid[(int'(rr_ptr_r) + arb_i) % 2]) begin
        arb_found = 1'b1;
        grant_valid = 1'b1;
        grant_requester = 1'((int'(rr_ptr_r) + arb_i) % 2);
        request_ready[(int'(rr_ptr_r) + arb_i) % 2] = 1'b1;
      end
    end
  end

endmodule

// ── Transfer FSM ──────────────────────────────────────────────────────────────
// Three-state machine: Idle → Running (read+write simultaneously) → Done → Idle.
// `active` gates both memory masters; `fire_irq` pulses for exactly one cycle.
module TransferFsm (
  input logic clk,
  input logic rst,
  input logic start,
  input logic all_done,
  output logic active,
  output logic fire_irq
);

  typedef enum logic [1:0] {
    IDLE = 2'd0,
    RUNNING = 2'd1,
    DONE = 2'd2
  } TransferFsm_state_t;
  
  TransferFsm_state_t state_r, state_next;
  
  always_ff @(posedge clk) begin
    if (rst) begin
      state_r <= IDLE;
    end else begin
      state_r <= state_next;
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      IDLE: begin
        if (start) state_next = RUNNING;
        else if ((!start)) state_next = IDLE;
      end
      RUNNING: begin
        if (all_done) state_next = DONE;
        else if ((!all_done)) state_next = RUNNING;
      end
      DONE: begin
        state_next = IDLE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    active = 1'b0; // default
    fire_irq = 1'b0; // default
    case (state_r)
      IDLE: begin
      end
      RUNNING: begin
        active = 1'b1;
      end
      DONE: begin
        fire_irq = 1'b1;
      end
      default: ;
    endcase
  end

endmodule

// Idle: both outputs stay at their defaults (false) — no comb block needed.
// override default; fire_irq stays false
// override default; active stays false
// ── DMA engine top-level module ───────────────────────────────────────────────
// Instantiates all five sub-constructs: DmaRegs (regfile), DescTable (ram),
// WriteBuffer (fifo), BeatCounter×2 (counter), and TransferFsm (fsm).
// DescTable is read during a 3-cycle fetch phase after start_pulse fires.
module DmaEngine #(
  parameter int DATA_WIDTH = 32
) (
  input logic clk,
  input logic rst,
  input logic apb_sel,
  input logic apb_enable,
  input logic apb_write,
  input logic [3-1:0] apb_addr,
  input logic [32-1:0] apb_wdata,
  output logic [32-1:0] apb_rdata,
  output logic apb_ready,
  output logic mem_rd_valid,
  input logic mem_rd_ready,
  output logic [32-1:0] mem_rd_addr,
  input logic [32-1:0] mem_rd_data,
  output logic mem_wr_valid,
  input logic mem_wr_ready,
  output logic [32-1:0] mem_wr_addr,
  output logic [32-1:0] mem_wr_data,
  output logic irq,
  output logic busy
);

  // ── APB slave (configuration) ──────────────────────────────────────────────
  // ── Memory read master ─────────────────────────────────────────────────────
  // ── Memory write master ────────────────────────────────────────────────────
  // ── Status and interrupt ───────────────────────────────────────────────────
  // ── Wires for DmaRegs read outputs (sole driver = inst connection) ─────────
  // Port 0 hardwired to addr 0 (src); port 1 hardwired to addr 1 (dst).
  logic [32-1:0] regs_src = 0;
  logic [32-1:0] regs_dst = 0;
  // ── Wires for BeatCounter outputs (write-beat counter) ────────────────────
  logic [8-1:0] beat_val = 0;
  logic at_max_r = 1'b0;
  // ── Wires for WriteBuffer FIFO outputs ────────────────────────────────────
  logic wb_push_ready = 1'b0;
  logic wb_pop_valid = 1'b0;
  logic [32-1:0] wb_pop_data = 0;
  // Read-beat counter wires (inst read_ctr: BeatCounter below).
  // Separate from beat_val/BeatCounter which tracks write beats.
  logic [8-1:0] read_val = 0;
  logic read_at_max_r = 1'b0;
  // Latch: set when last read fires, cleared when FSM returns to Idle.
  logic reads_done_r = 1'b0;
  // ── Descriptor fetch state registers ──────────────────────────────────────
  // desc_rdata: registered output of DescTable read port (1-cycle sync latency).
  // fetch_cnt:  0=idle, 1=reading[0]/src, 2=reading[1]/dst+latch src, 3=done.
  // Transfer is gated until fetch_cnt reaches 3.
  logic [32-1:0] desc_rdata = 0;
  logic [2-1:0] fetch_cnt = 0;
  // Fetched descriptor: src/dst loaded from DescTable; length written via APB.
  DmaDescriptor desc_r = '{src_addr: 0, dst_addr: 0, length: 0, flags: 0};
  // ── Wires for MemArbiter outputs ──────────────────────────────────────────
  // request_ready is a 2-bit packed vector: bit[0]=read granted, bit[1]=write granted.
  logic [2-1:0] arb_req_ready = 0;
  logic arb_grant_valid = 1'b0;
  logic [1-1:0] arb_grant_req = 0;
  // ── Let bindings ──────────────────────────────────────────────────────────
  // start_pulse: one-cycle high when APB writes '1' to the start register
  logic start_pulse;
  assign start_pulse = ((((apb_sel && apb_enable) && apb_write) && (apb_addr == 3)) && (apb_wdata == 1));
  // desc_rd_en: issue a read during fetch cycles 1 (word 0) and 2 (word 1)
  logic desc_rd_en;
  assign desc_rd_en = ((fetch_cnt == 1) || (fetch_cnt == 2));
  // desc_rd_addr: address 0 (src) during cycle 1, address 1 (dst) during cycle 2
  logic [4-1:0] desc_rd_addr;
  assign desc_rd_addr = 4'((fetch_cnt == 2));
  // Arbiter grant signals (bits of arb_req_ready packed vector)
  logic read_granted;
  assign read_granted = arb_req_ready[0];
  logic write_granted;
  assign write_granted = arb_req_ready[1];
  // bus_cmd: typed view of the arbitrated bus transaction each cycle
  BusCmd bus_cmd;
  assign bus_cmd = ((arb_req_ready == 'b1) ? BUSREAD : ((arb_req_ready == 'b10) ? BUSWRITE : BUSIDLE));
  // read_wants / write_wants: pre-grant bus request signals fed to MemArbiter
  logic read_wants;
  assign read_wants = (((busy && wb_push_ready) && (!reads_done_r)) && (fetch_cnt == 3));
  logic write_wants;
  assign write_wants = wb_pop_valid;
  // read_fired: read handshake accepted; mem_rd_valid already includes read_granted
  logic read_fired;
  assign read_fired = (mem_rd_valid && mem_rd_ready);
  // read_done: last read beat just completed
  logic read_done;
  assign read_done = (read_fired && (16'(read_val) == desc_r.length));
  // beat_fired: write handshake accepted; mem_wr_valid already includes write_granted
  logic beat_fired;
  assign beat_fired = (mem_wr_valid && mem_wr_ready);
  // all_done: last write beat completed — triggers FSM→Done and counter clear
  logic all_done;
  assign all_done = (beat_fired && (16'(beat_val) == desc_r.length));
  // ── CSR register file instance ────────────────────────────────────────────
  DmaRegs regs (
    .clk(clk),
    .rst(rst),
    .read0_addr(0),
    .read0_data(regs_src),
    .read1_addr(1),
    .read1_data(regs_dst),
    .write_en(((apb_sel && apb_enable) && apb_write)),
    .write_addr(apb_addr),
    .write_data(apb_wdata)
  );
  // ── Transfer FSM instance ─────────────────────────────────────────────────
  TransferFsm ctrl (
    .clk(clk),
    .rst(rst),
    .start(start_pulse),
    .all_done(all_done),
    .active(busy),
    .fire_irq(irq)
  );
  // ── Read-beat counter instance ────────────────────────────────────────────
  // Counts read handshakes; cleared when last read fires (read_done).
  BeatCounter read_ctr (
    .clk(clk),
    .rst(rst),
    .inc(read_fired),
    .clear(read_done),
    .value(read_val),
    .at_max(read_at_max_r)
  );
  // ── Write-beat counter instance ───────────────────────────────────────────
  // Counts FIFO pops (write beats); cleared when last write completes.
  BeatCounter beat (
    .clk(clk),
    .rst(rst),
    .inc(beat_fired),
    .clear(all_done),
    .value(beat_val),
    .at_max(at_max_r)
  );
  // ── Write buffer FIFO instance ────────────────────────────────────────────
  // Push: fires when read handshake completes.
  // Pop:  fires when write bus is ready (mem_wr_ready).
  // Result: write data is always the read data from the preceding beat.
  WriteBuffer wb (
    .clk(clk),
    .rst(rst),
    .push_valid(read_fired),
    .push_ready(wb_push_ready),
    .push_data(mem_rd_data),
    .pop_valid(wb_pop_valid),
    .pop_ready((mem_wr_ready && write_granted)),
    .pop_data(wb_pop_data)
  );
  // ── Descriptor table RAM instance ─────────────────────────────────────────
  // Write port: APB programs descriptor words 0 (src) and 1 (dst) before start.
  // Read port:  fetch state machine reads src/dst on the two cycles after start.
  DescTable desc (
    .clk(clk),
    .rd_port_addr(desc_rd_addr),
    .rd_port_en(desc_rd_en),
    .rd_port_rdata(desc_rdata),
    .wr_port_addr(4'(apb_addr)),
    .wr_port_en(((apb_sel && apb_enable) && apb_write)),
    .wr_port_wen(((apb_sel && apb_enable) && apb_write)),
    .wr_port_wdata(apb_wdata)
  );
  // ── Memory bus arbiter instance ───────────────────────────────────────────
  // Requester 0 = read master; requester 1 = write master.
  // {write_wants, read_wants} packs them into the 2-bit request_valid vector
  // (MSB = requester 1 = write, LSB = requester 0 = read).
  MemArbiter arb (
    .clk(clk),
    .rst(rst),
    .request_valid({write_wants, read_wants}),
    .request_ready(arb_req_ready),
    .grant_valid(arb_grant_valid),
    .grant_requester(arb_grant_req)
  );
  // ── All-reads-done latch: set when last read fires, clear on FSM idle ──────
  always_ff @(posedge clk) begin
    if (rst) begin
      reads_done_r <= 1'b0;
    end else begin
      if ((!busy)) begin
        reads_done_r <= 1'b0;
      end else begin
        if (read_done) begin
          reads_done_r <= 1'b1;
        end
      end
    end
  end
  // ── Descriptor fetch state machine ────────────────────────────────────────
  // start_pulse: fetch_cnt 0→1.  Each subsequent cycle advances by one.
  // Cycle 1: DescTable[0] read issued.
  // Cycle 2: DescTable[0] output available → latch desc_src_r; read [1] issued.
  // Cycle 3: DescTable[1] output available → latch desc_dst_r; fetch complete.
  // all_done: resets fetch_cnt to 0, ready for next transfer.
  always_ff @(posedge clk) begin
    if (rst) begin
      desc_r <= '{src_addr: 0, dst_addr: 0, length: 0, flags: 0};
      fetch_cnt <= 0;
    end else begin
      if (start_pulse) begin
        fetch_cnt <= 1;
      end
      if ((fetch_cnt == 1)) begin
        fetch_cnt <= 2;
      end
      if ((fetch_cnt == 2)) begin
        desc_r.src_addr <= desc_rdata;
        fetch_cnt <= 3;
      end
      if ((fetch_cnt == 3)) begin
        desc_r.dst_addr <= desc_rdata;
      end
      if (all_done) begin
        fetch_cnt <= 0;
      end
    end
  end
  // ── APB write: latch desc_r.length (and desc_r.flags if addr 4 used) ───────
  always_ff @(posedge clk) begin
    if (rst) begin
      desc_r <= '{src_addr: 0, dst_addr: 0, length: 0, flags: 0};
    end else begin
      if (((apb_sel && apb_enable) && apb_write)) begin
        if ((apb_addr == 2)) begin
          desc_r.length <= 16'(apb_wdata);
        end
      end
    end
  end
  // ── Combinational outputs ─────────────────────────────────────────────────
  // mem_rd_valid: read only while FSM active, FIFO has space, reads not done.
  // mem_wr_valid: write whenever FIFO has data (wb_pop_valid).
  // Addresses use separate counters so reads run one beat ahead of writes.
  always_comb begin
    mem_rd_valid = (bus_cmd == BUSREAD);
    mem_rd_addr = (desc_r.src_addr + 32'(read_val));
    mem_wr_valid = (bus_cmd == BUSWRITE);
    mem_wr_addr = (desc_r.dst_addr + 32'(beat_val));
    mem_wr_data = wb_pop_data;
    apb_ready = 1'b1;
    case (apb_addr)
      0: apb_rdata = regs_src;
      1: apb_rdata = regs_dst;
      2: apb_rdata = 32'(beat_val);
      default: apb_rdata = 0;
    endcase
  end

endmodule

// Drive valid signals from the typed bus command (arbiter grant encoded as BusCmd).
