// domain SysDomain
//   freq_mhz: 100

module LifoStack #(
  parameter int  DEPTH = 16,
  parameter type TYPE  = logic [8-1:0]
) (
  input logic clk,
  input logic rst,
  input logic push_valid,
  output logic push_ready,
  input TYPE push_data,
  output logic pop_valid,
  input logic pop_ready,
  output TYPE pop_data,
  output logic full,
  output logic empty
);

  localparam int PTR_W = $clog2(DEPTH + 1);
  
  TYPE                  mem [0:DEPTH-1];
  logic [PTR_W-1:0]     sp;
  
  assign full        = (sp == DEPTH[PTR_W-1:0]);
  assign empty       = (sp == '0);
  assign push_ready  = !full;
  assign pop_valid   = !empty;
  assign pop_data    = mem[sp - 1];
  
  always_ff @(posedge clk) begin
    if (rst) begin
      sp <= '0;
    end else begin
      if (push_valid && push_ready && pop_valid && pop_ready) begin
        // Simultaneous push+pop: replace top of stack
        mem[sp - 1] <= push_data;
      end else if (push_valid && push_ready) begin
        mem[sp] <= push_data;
        sp <= sp + 1;
      end else if (pop_valid && pop_ready) begin
        sp <= sp - 1;
      end
    end
  end

endmodule

