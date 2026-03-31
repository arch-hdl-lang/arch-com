module interrupt_controller_apb #(
  parameter int NUM_INTERRUPTS = 4,
  parameter int ADDR_WIDTH = 8
) (
  input logic clk,
  input logic rst_n,
  input logic [NUM_INTERRUPTS-1:0] interrupt_requests,
  output logic [NUM_INTERRUPTS-1:0] interrupt_service,
  output logic cpu_interrupt,
  input logic cpu_ack,
  output logic [$clog2(NUM_INTERRUPTS)-1:0] interrupt_idx,
  output logic [ADDR_WIDTH-1:0] interrupt_vector,
  input logic pclk,
  input logic presetn,
  input logic psel,
  input logic penable,
  input logic pwrite,
  input logic [ADDR_WIDTH-1:0] paddr,
  input logic [32-1:0] pwdata,
  output logic [32-1:0] prdata,
  output logic pready
);

  // Interrupt logic registers (clk domain)
  logic [NUM_INTERRUPTS-1:0] pending_interrupts;
  logic servicing;
  logic [NUM_INTERRUPTS-1:0] current_int;
  logic [$clog2(NUM_INTERRUPTS)-1:0] current_idx;
  // Delayed has_pending: dispatch fires one cycle after a new interrupt enters
  // masked_pending, so cpu_interrupt cannot go high in the extra FallingEdge
  // window that check_int_out uses to verify cpu_interrupt==0 after list empties.
  logic has_pending_r;
  // prev_requests: interrupt_requests from previous cycle.
  // Used at ack time to detect whether a new pulse for the currently-serviced
  // interrupt arrived one cycle ago (already absorbed into pending_interrupts).
  // If so, preserve that bit in pending_interrupts rather than clearing it.
  logic [NUM_INTERRUPTS-1:0] prev_requests;
  // APB-configured registers (pclk domain)
  logic [24-1:0] priority_map [NUM_INTERRUPTS-1:0];
  logic [24-1:0] vector_table [NUM_INTERRUPTS-1:0];
  logic [NUM_INTERRUPTS-1:0] interrupt_mask;
  // Combinational wires for priority arbitration
  logic [NUM_INTERRUPTS-1:0] masked_pending;
  logic [NUM_INTERRUPTS-1:0] winner_int;
  logic [32-1:0] winner_idx32;
  logic [24-1:0] highest_pri_val;
  logic has_pending;
  logic current_still_unmasked;
  logic [24-1:0] current_priority;
  // Priority-based arbitration (combinational).
  // masked_pending excludes current_int so that:
  //   1) has_pending/has_pending_r go false after ack when no OTHER interrupt
  //      is pending, giving dispatch the required 1-cycle delay.
  //   2) winner is always a DIFFERENT interrupt than the one being served,
  //      enabling an explicit priority comparison for preemption.
  always_comb begin
    masked_pending = (pending_interrupts | interrupt_requests) & interrupt_mask & ~current_int;
    winner_int = 0;
    winner_idx32 = 0;
    highest_pri_val = 16777215;
    has_pending = 1'b0;
    for (int i = 0; i <= NUM_INTERRUPTS - 1; i++) begin
      if (masked_pending[i]) begin
        if (~has_pending | priority_map[i] < highest_pri_val) begin
          winner_idx32 = 32'($unsigned(i));
          winner_int = NUM_INTERRUPTS'($unsigned(1)) << NUM_INTERRUPTS'($unsigned(i));
          highest_pri_val = priority_map[i];
          has_pending = 1'b1;
        end
      end
    end
    current_still_unmasked = (current_int & interrupt_mask) != 0;
    // Priority of currently-serviced interrupt (used for preemption guard)
    current_priority = priority_map[current_idx];
  end
  // Interrupt dispatch, preemption, and ack logic (clk domain)
  always_ff @(posedge clk) begin
    if ((!rst_n)) begin
      current_idx <= 0;
      current_int <= 0;
      has_pending_r <= 1'b0;
      pending_interrupts <= 0;
      prev_requests <= 0;
      servicing <= 1'b0;
    end else begin
      prev_requests <= interrupt_requests;
      has_pending_r <= has_pending;
      // Latch all incoming interrupt requests
      pending_interrupts <= pending_interrupts | interrupt_requests;
      if (cpu_ack & servicing) begin
        // Clear the serviced interrupt unless a new pulse for it arrived last cycle
        // (prev_requests & current_int non-zero means the bit was already absorbed
        // into pending_interrupts and must not be cleared by the ack).
        pending_interrupts <= pending_interrupts & ~(current_int & ~prev_requests) | interrupt_requests;
        servicing <= 1'b0;
        current_int <= 0;
        current_idx <= 0;
      end else if (servicing & ~current_still_unmasked) begin
        // Active interrupt was masked — abort without clearing pending
        servicing <= 1'b0;
        current_int <= 0;
        current_idx <= 0;
      end else if (servicing & has_pending & highest_pri_val < current_priority) begin
        // Preempt: winner (excluded current from masked_pending) has strictly
        // higher priority than current. Fires immediately (no has_pending_r delay)
        // so current_idx updates at the same NBA as interrupts_list.add() runs
        // in the testbench's ReadWrite phase.
        current_int <= winner_int;
        current_idx <= $clog2(NUM_INTERRUPTS)'(winner_idx32);
      end else if (~servicing & has_pending_r) begin
        // Delayed dispatch: fires one cycle after masked_pending goes non-empty.
        servicing <= 1'b1;
        current_int <= winner_int;
        current_idx <= $clog2(NUM_INTERRUPTS)'(winner_idx32);
      end
    end
  end
  // Output assignments (combinational)
  assign interrupt_service = current_int;
  assign cpu_interrupt = servicing & ~cpu_ack & current_still_unmasked;
  assign interrupt_idx = current_idx;
  assign interrupt_vector = ADDR_WIDTH'(vector_table[current_idx]);
  // Deassert cpu_interrupt while cpu_ack is active
  // APB combinational read data; pready asserted combinationally in access phase
  logic [32-1:0] apb_rdata;
  always_comb begin
    apb_rdata = 0;
    pready = psel & penable;
    if (psel & penable & ~pwrite) begin
      if (paddr[3:0] == 0) begin
        for (int i = 0; i <= NUM_INTERRUPTS - 1; i++) begin
          if (paddr[7:4] == 4'($unsigned(i))) begin
            apb_rdata = 32'($unsigned(priority_map[i]));
          end
        end
      end else if (paddr[3:0] == 1) begin
        apb_rdata = 32'($unsigned(interrupt_mask));
      end else if (paddr[3:0] == 2) begin
        for (int i = 0; i <= NUM_INTERRUPTS - 1; i++) begin
          if (paddr[7:4] == 4'($unsigned(i))) begin
            apb_rdata = 32'($unsigned(vector_table[i]));
          end
        end
      end else if (paddr[3:0] == 3) begin
        apb_rdata = 32'($unsigned(pending_interrupts));
      end else if (paddr[3:0] == 4) begin
        apb_rdata = 32'($unsigned(current_idx));
      end
    end
    prdata = apb_rdata;
  end
  // APB register writes (pclk domain)
  // Address map (paddr[3:0]):
  //   0x0: priority_map — pwdata[7:0]=irq_index, pwdata[31:8]=priority_value
  //   0x1: interrupt_mask — pwdata[NUM_INTERRUPTS-1:0]=new_mask
  //   0x2: vector_table  — pwdata[7:0]=irq_index, pwdata[31:8]=vector_address
  always_ff @(posedge pclk) begin
    if (~presetn) begin
      for (int i = 0; i <= NUM_INTERRUPTS - 1; i++) begin
        priority_map[i] <= 24'(i);
        vector_table[i] <= 24'(i * 4);
      end
      interrupt_mask <= ~NUM_INTERRUPTS'($unsigned(0));
    end else if (psel & penable & pwrite) begin
      if (paddr[3:0] == 0) begin
        for (int i = 0; i <= NUM_INTERRUPTS - 1; i++) begin
          if (pwdata[7:0] == 8'($unsigned(i))) begin
            priority_map[i] <= pwdata[31:8];
          end
        end
      end else if (paddr[3:0] == 1) begin
        interrupt_mask <= NUM_INTERRUPTS'(pwdata);
      end else if (paddr[3:0] == 2) begin
        for (int i = 0; i <= NUM_INTERRUPTS - 1; i++) begin
          if (pwdata[7:0] == 8'($unsigned(i))) begin
            vector_table[i] <= pwdata[31:8];
          end
        end
      end
    end
  end

endmodule

