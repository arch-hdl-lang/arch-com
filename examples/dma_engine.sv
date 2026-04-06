// =============================================================================
// Simple DMA Engine
// Demonstrates: domain, struct, enum, regfile, ram, fifo, counter,
//               arbiter, fsm, module (comb + reg blocks, match, inst)
// =============================================================================
// ── Clock domain ─────────────────────────────────────────────────────────────
// domain SysDomain
//   freq_mhz: 100

// ── Shared types ──────────────────────────────────────────────────────────────
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

// ── CSR register file ─────────────────────────────────────────────────────────
module DmaRegs #(
  parameter int NREGS = 8,
  parameter int WIDTH = 0
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

  logic [32-1:0] rf_data [0:NREGS-1];
  
  always_ff @(posedge clk) begin
    if (write_en)
      rf_data[write_addr] <= write_data;
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

// ── Descriptor table RAM ──────────────────────────────────────────────────────
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

// ── Write-side data FIFO ──────────────────────────────────────────────────────
module WriteBuffer #(
  parameter int  DEPTH = 8,
  parameter type TYPE  = logic [32-1:0]
) (
  input logic clk,
  input logic rst,
  input logic push_valid,
  output logic push_ready,
  input TYPE push_data,
  output logic pop_valid,
  input logic pop_ready,
  output TYPE pop_data
);

  localparam int PTR_W = $clog2(DEPTH) + 1;
  
  TYPE                  mem [0:DEPTH-1];
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

// ── Beat counter ──────────────────────────────────────────────────────────────
module BeatCounter #(
  parameter int MAX = 255
) (
  input logic clk,
  input logic rst,
  input logic inc,
  input logic clear,
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

// ── Memory bus arbiter ────────────────────────────────────────────────────────
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
      active <= 1'b0;
      fire_irq <= 1'b0;
      case (state_r)
        RUNNING: begin
          active <= 1'b1;
        end
        DONE: begin
          fire_irq <= 1'b1;
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      IDLE: begin
        if (start) state_next = RUNNING;
      end
      RUNNING: begin
        if (all_done) state_next = DONE;
      end
      DONE: begin
        state_next = IDLE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    case (state_r)
      IDLE: begin
      end
      RUNNING: begin
      end
      DONE: begin
      end
      default: ;
    endcase
  end

endmodule

// ── DMA engine top-level module ───────────────────────────────────────────────
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

  // ── APB slave ──────────────────────────────────────────────────────────────
  // ── Memory read master ─────────────────────────────────────────────────────
  // ── Memory write master ────────────────────────────────────────────────────
  // ── Status and interrupt ───────────────────────────────────────────────────
  // ── Instance output wires ─────────────────────────────────────────────────
  logic [32-1:0] regs_src = 0;
  logic [32-1:0] regs_dst = 0;
  logic [8-1:0] beat_val = 0;
  logic at_max_r = 1'b0;
  logic wb_push_ready = 1'b0;
  logic wb_pop_valid = 1'b0;
  logic [32-1:0] wb_pop_data = 0;
  logic [8-1:0] read_val = 0;
  logic read_at_max_r = 1'b0;
  logic reads_done_r;
  // ── Descriptor fetch state ────────────────────────────────────────────────
  logic [32-1:0] desc_rdata = 0;
  logic [2-1:0] fetch_cnt;
  DmaDescriptor desc_r = '{src_addr: 0, dst_addr: 0, length: 0, flags: 0};
  // ── Arbiter output wires ──────────────────────────────────────────────────
  logic [2-1:0] arb_req_ready = 0;
  logic arb_grant_valid = 1'b0;
  logic [1-1:0] arb_grant_req = 0;
  // ── Let bindings ──────────────────────────────────────────────────────────
  logic start_pulse;
  assign start_pulse = apb_sel & apb_enable & apb_write & apb_addr == 3 & apb_wdata == 1;
  logic desc_rd_en;
  assign desc_rd_en = fetch_cnt == 1 | fetch_cnt == 2;
  logic [4-1:0] desc_rd_addr;
  assign desc_rd_addr = 4'($unsigned(fetch_cnt == 2));
  logic read_granted;
  assign read_granted = arb_req_ready[0];
  logic write_granted;
  assign write_granted = arb_req_ready[1];
  BusCmd bus_cmd;
  assign bus_cmd = ((arb_req_ready == 'b1) ? BUSREAD : ((arb_req_ready == 'b10) ? BUSWRITE : BUSIDLE));
  logic read_wants;
  assign read_wants = busy & wb_push_ready & ~reads_done_r & fetch_cnt == 3;
  logic write_wants;
  assign write_wants = wb_pop_valid;
  logic read_fired;
  assign read_fired = mem_rd_valid & mem_rd_ready;
  logic read_done;
  assign read_done = read_fired & 16'($unsigned(read_val)) == desc_r.length;
  logic beat_fired;
  assign beat_fired = mem_wr_valid & mem_wr_ready;
  logic all_done;
  assign all_done = beat_fired & 16'($unsigned(beat_val)) == desc_r.length;
  // ── Instances ────────────────────────────────────────────────────────────
  DmaRegs regs (
    .clk(clk),
    .rst(rst),
    .read0_addr(0),
    .read0_data(regs_src),
    .read1_addr(1),
    .read1_data(regs_dst),
    .write_en(apb_sel & apb_enable & apb_write),
    .write_addr(apb_addr),
    .write_data(apb_wdata)
  );
  TransferFsm ctrl (
    .clk(clk),
    .rst(rst),
    .start(start_pulse),
    .all_done(all_done),
    .active(busy),
    .fire_irq(irq)
  );
  BeatCounter read_ctr (
    .clk(clk),
    .rst(rst),
    .inc(read_fired),
    .clear(read_done),
    .value(read_val),
    .at_max(read_at_max_r)
  );
  BeatCounter beat (
    .clk(clk),
    .rst(rst),
    .inc(beat_fired),
    .clear(all_done),
    .value(beat_val),
    .at_max(at_max_r)
  );
  WriteBuffer wb (
    .clk(clk),
    .rst(rst),
    .push_valid(read_fired),
    .push_ready(wb_push_ready),
    .push_data(mem_rd_data),
    .pop_valid(wb_pop_valid),
    .pop_ready(mem_wr_ready & write_granted),
    .pop_data(wb_pop_data)
  );
  DescTable desc (
    .clk(clk),
    .rd_port_addr(desc_rd_addr),
    .rd_port_en(desc_rd_en),
    .rd_port_rdata(desc_rdata),
    .wr_port_addr(4'($unsigned(apb_addr))),
    .wr_port_en(apb_sel & apb_enable & apb_write),
    .wr_port_wen(apb_sel & apb_enable & apb_write),
    .wr_port_wdata(apb_wdata)
  );
  MemArbiter arb (
    .clk(clk),
    .rst(rst),
    .request_valid({write_wants, read_wants}),
    .request_ready(arb_req_ready),
    .grant_valid(arb_grant_valid),
    .grant_requester(arb_grant_req)
  );
  // ── All-reads-done latch ───────────────────────────────────────────────────
  always_ff @(posedge clk) begin
    if (rst) begin
      reads_done_r <= 1'b0;
    end else begin
      if (~busy) begin
        reads_done_r <= 1'b0;
      end else if (read_done) begin
        reads_done_r <= 1'b1;
      end
    end
  end
  // ── Descriptor fetch state machine ────────────────────────────────────────
  always_ff @(posedge clk) begin
    if (rst) begin
      fetch_cnt <= 0;
    end else begin
      if (start_pulse) begin
        fetch_cnt <= 1;
      end
      if (fetch_cnt == 1) begin
        fetch_cnt <= 2;
      end
      if (fetch_cnt == 2) begin
        desc_r.src_addr <= desc_rdata;
        fetch_cnt <= 3;
      end
      if (all_done) begin
        fetch_cnt <= 0;
      end
    end
  end
  always_ff @(posedge clk) begin
    if (fetch_cnt == 3) begin
      desc_r.dst_addr <= desc_rdata;
    end
  end
  // ── APB write: latch desc_r.length ────────────────────────────────────────
  always_ff @(posedge clk) begin
    if (apb_sel & apb_enable & apb_write) begin
      if (apb_addr == 2) begin
        desc_r.length <= 16'(apb_wdata);
      end
    end
  end
  // ── Combinational outputs ─────────────────────────────────────────────────
  always_comb begin
    mem_rd_valid = bus_cmd == BUSREAD;
    mem_rd_addr = 32'(desc_r.src_addr + 32'($unsigned(read_val)));
    mem_wr_valid = bus_cmd == BUSWRITE;
    mem_wr_addr = 32'(desc_r.dst_addr + 32'($unsigned(beat_val)));
    mem_wr_data = wb_pop_data;
    apb_ready = 1'b1;
    case (apb_addr)
      0: apb_rdata = regs_src;
      1: apb_rdata = regs_dst;
      2: apb_rdata = 32'($unsigned(beat_val));
      default: apb_rdata = 0;
    endcase
  end

endmodule

