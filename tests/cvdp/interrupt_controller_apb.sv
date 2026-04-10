module interrupt_controller_apb #(
  parameter int NUM_INTERRUPTS = 4,
  parameter int ADDR_WIDTH = 8,
  parameter int IDX_WIDTH = $clog2(NUM_INTERRUPTS) > 0 ? $clog2(NUM_INTERRUPTS) : 1
) (
  input logic clk,
  input logic rst_n,
  input logic [NUM_INTERRUPTS-1:0] interrupt_requests,
  output logic [NUM_INTERRUPTS-1:0] interrupt_service,
  output logic cpu_interrupt,
  input logic cpu_ack,
  output logic [IDX_WIDTH-1:0] interrupt_idx,
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
  logic [IDX_WIDTH-1:0] current_idx;
  logic has_pending_r;
  // APB-configured registers (pclk domain)
  logic [24-1:0] priority_map [NUM_INTERRUPTS-1:0];
  logic [24-1:0] vector_table [NUM_INTERRUPTS-1:0];
  logic [NUM_INTERRUPTS-1:0] interrupt_mask;
  // Combinational wires for priority arbitration
  logic [NUM_INTERRUPTS-1:0] effective_pending;
  logic [NUM_INTERRUPTS-1:0] masked_pending;
  logic [NUM_INTERRUPTS-1:0] winner_int;
  logic [32-1:0] winner_idx32;
  logic [24-1:0] highest_pri_val;
  logic has_pending;
  logic current_still_unmasked;
  // Priority-based arbitration (combinational).
  // Winner uses effective_pending (pending | requests) for same-cycle visibility.
  // has_pending uses only pending_interrupts for dispatch timing.
  always_comb begin
    effective_pending = pending_interrupts | interrupt_requests;
    masked_pending = effective_pending & interrupt_mask;
    winner_int = 0;
    winner_idx32 = 0;
    highest_pri_val = 16777215;
    for (int i = 0; i <= NUM_INTERRUPTS - 1; i++) begin
      if (masked_pending[i]) begin
        if (priority_map[i] < highest_pri_val) begin
          winner_idx32 = 32'($unsigned(i));
          winner_int = NUM_INTERRUPTS'($unsigned(1)) << NUM_INTERRUPTS'($unsigned(i));
          highest_pri_val = priority_map[i];
        end
      end
    end
    has_pending = (pending_interrupts & interrupt_mask) != 0;
    current_still_unmasked = (current_int & interrupt_mask) != 0;
  end
  // Interrupt dispatch and ack logic (clk domain)
  always_ff @(posedge clk) begin
    if ((!rst_n)) begin
      current_idx <= 0;
      current_int <= 0;
      has_pending_r <= 1'b0;
      pending_interrupts <= 0;
      servicing <= 1'b0;
    end else begin
      // Always update the delayed dispatch enable
      has_pending_r <= has_pending;
      // Latch all incoming interrupt requests
      pending_interrupts <= pending_interrupts | interrupt_requests;
      if (cpu_ack & servicing) begin
        // Clear the serviced interrupt's pending bit
        pending_interrupts <= pending_interrupts & ~current_int | interrupt_requests;
        servicing <= 1'b0;
        current_int <= 0;
        current_idx <= 0;
      end else if (servicing & ~current_still_unmasked) begin
        servicing <= 1'b0;
        current_int <= 0;
        current_idx <= 0;
      end else if (~servicing & has_pending_r & has_pending) begin
        // Dispatch: requires both delayed AND current has_pending
        servicing <= 1'b1;
        current_int <= winner_int;
        current_idx <= IDX_WIDTH'(winner_idx32);
      end
    end
  end
  // Output assignments (combinational)
  assign interrupt_service = current_int;
  assign cpu_interrupt = servicing & ~cpu_ack & current_still_unmasked;
  assign interrupt_idx = current_idx;
  assign interrupt_vector = ADDR_WIDTH'(vector_table[current_idx]);
  // APB combinational read data
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

