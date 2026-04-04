module interrupt_controller #(
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
  input logic [NUM_INTERRUPTS-1:0] priority_map_value [NUM_INTERRUPTS-1:0],
  input logic priority_map_update,
  input logic [ADDR_WIDTH-1:0] vector_table_value [NUM_INTERRUPTS-1:0],
  input logic vector_table_update,
  input logic [NUM_INTERRUPTS-1:0] interrupt_mask_value,
  input logic interrupt_mask_update
);

  logic [NUM_INTERRUPTS-1:0] all_ones_n;
  assign all_ones_n = ~NUM_INTERRUPTS'($unsigned(0));
  logic [NUM_INTERRUPTS-1:0] one_wide;
  assign one_wide = NUM_INTERRUPTS'($unsigned(1));
  // Complex reset values — opt out of auto-reset; handled manually in seq
  logic [NUM_INTERRUPTS-1:0] priority_map [NUM_INTERRUPTS-1:0];
  logic [ADDR_WIDTH-1:0] vector_table [NUM_INTERRUPTS-1:0];
  logic [NUM_INTERRUPTS-1:0] interrupt_mask;
  logic [NUM_INTERRUPTS-1:0] pending_interrupts;
  logic [NUM_INTERRUPTS-1:0] sync_requests_0;
  logic [NUM_INTERRUPTS-1:0] sync_requests_1;
  logic [NUM_INTERRUPTS-1:0] r_interrupt_service;
  logic r_cpu_interrupt;
  logic [$clog2(NUM_INTERRUPTS)-1:0] r_interrupt_idx;
  logic [ADDR_WIDTH-1:0] r_interrupt_vector;
  logic servicing;
  logic [$clog2(NUM_INTERRUPTS)-1:0] highest_pri_idx;
  logic [NUM_INTERRUPTS-1:0] highest_pri_val;
  logic has_pending;
  logic [NUM_INTERRUPTS-1:0] masked_pending;
  assign interrupt_service = r_interrupt_service;
  assign cpu_interrupt = r_cpu_interrupt;
  assign interrupt_idx = r_interrupt_idx;
  assign interrupt_vector = r_interrupt_vector;
  assign masked_pending = pending_interrupts & interrupt_mask;
  always_comb begin
    highest_pri_idx = 0;
    highest_pri_val = all_ones_n;
    has_pending = 1'b0;
    for (int i = 0; i <= NUM_INTERRUPTS - 1; i++) begin
      if (masked_pending[i]) begin
        if (~has_pending | priority_map[i] < highest_pri_val) begin
          highest_pri_idx = $clog2(NUM_INTERRUPTS)'(i);
          highest_pri_val = priority_map[i];
          has_pending = 1'b1;
        end
      end
    end
  end
  always_ff @(posedge clk) begin
    if ((!rst_n)) begin
      sync_requests_0 <= 0;
      sync_requests_1 <= 0;
    end else begin
      sync_requests_0 <= interrupt_requests;
      sync_requests_1 <= sync_requests_0;
    end
  end
  // Separate seq for complex-reset regs (no auto-reset guard)
  always_ff @(posedge clk) begin
    if (~rst_n) begin
      for (int i = 0; i <= NUM_INTERRUPTS - 1; i++) begin
        priority_map[i] <= NUM_INTERRUPTS'(i);
        vector_table[i] <= ADDR_WIDTH'(i * 4);
      end
      interrupt_mask <= all_ones_n;
    end else begin
      if (priority_map_update) begin
        for (int i = 0; i <= NUM_INTERRUPTS - 1; i++) begin
          priority_map[i] <= priority_map_value[i];
        end
      end
      if (vector_table_update) begin
        for (int i = 0; i <= NUM_INTERRUPTS - 1; i++) begin
          vector_table[i] <= vector_table_value[i];
        end
      end
      if (interrupt_mask_update) begin
        interrupt_mask <= interrupt_mask_value;
      end
    end
  end
  // Separate seq for simple-reset regs (auto-reset guard handles reset)
  always_ff @(posedge clk) begin
    if ((!rst_n)) begin
      pending_interrupts <= 0;
      r_cpu_interrupt <= 1'b0;
      r_interrupt_idx <= 0;
      r_interrupt_service <= 0;
      r_interrupt_vector <= 0;
      servicing <= 1'b0;
    end else begin
      pending_interrupts <= pending_interrupts | sync_requests_1;
      if (cpu_ack) begin
        r_cpu_interrupt <= 1'b0;
        r_interrupt_service <= 0;
        pending_interrupts <= (pending_interrupts | sync_requests_1) & ~r_interrupt_service;
        servicing <= 1'b0;
      end else if (has_pending & ~servicing) begin
        r_cpu_interrupt <= 1'b1;
        r_interrupt_idx <= highest_pri_idx;
        r_interrupt_vector <= vector_table[highest_pri_idx];
        r_interrupt_service <= one_wide << highest_pri_idx;
        servicing <= 1'b1;
      end
    end
  end

endmodule

