// FILO_RTL — wraps the built-in `fifo kind: lifo` to match the CVDP
// testbench's port shape (`push`/`pop`/`data_in`/`data_out`/`full`/`empty`)
// and same-cycle feedthrough semantics on push+pop into an empty stack.
//
// Race avoidance: the inst's `push_valid` / `pop_ready` inputs are
// driven by *inlined* expressions (`push & ~(push & pop & empty)`),
// not via an intermediate `feedthrough` wire. iverilog can otherwise
// schedule the two continuous assigns out of order, sampling
// `push_valid` before `feedthrough` propagates the new `push` value
// — letting a spurious push slip through on the feedthrough cycle.
module FiloStack #(
  parameter int  DEPTH      = 16,
  parameter int  DATA_WIDTH = 8
) (
  input logic clk,
  input logic reset,
  input logic push_valid,
  output logic push_ready,
  input logic [DATA_WIDTH-1:0] push_data,
  output logic pop_valid,
  input logic pop_ready,
  output logic [DATA_WIDTH-1:0] pop_data
);

  localparam int PTR_W = $clog2(DEPTH + 1);
  
  logic [DATA_WIDTH-1:0] mem [0:DEPTH-1];
  logic [PTR_W-1:0]     sp;
  logic                 full;
  logic                 empty;
  
  assign full        = (sp == DEPTH[PTR_W-1:0]);
  assign empty       = (sp == '0);
  assign push_ready  = !full;
  assign pop_valid   = !empty;
  assign pop_data    = mem[sp - 1];
  
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
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
  
  // synopsys translate_off
  _auto_no_overflow: assert property (@(posedge clk) !(push_valid && push_ready && full))
    else $fatal(1, "FIFO OVERFLOW: FiloStack.push while full");
  _auto_no_underflow: assert property (@(posedge clk) !(pop_valid && pop_ready && empty))
    else $fatal(1, "FIFO UNDERFLOW: FiloStack.pop while empty");
  // synopsys translate_on

endmodule

module FILO_RTL #(
  parameter int DATA_WIDTH = 8,
  parameter int FILO_DEPTH = 16
) (
  input logic clk,
  input logic reset,
  input logic push,
  input logic pop,
  input logic [DATA_WIDTH-1:0] data_in,
  output logic [DATA_WIDTH-1:0] data_out,
  output logic full,
  output logic empty
);

  logic push_ready_w;
  logic pop_valid_w;
  logic [DATA_WIDTH-1:0] pop_data_w;
  FiloStack #(.DEPTH(FILO_DEPTH), .DATA_WIDTH(DATA_WIDTH)) stack (
    .clk(clk),
    .reset(reset),
    .push_valid(push & ~(push & pop & empty)),
    .push_data(data_in),
    .push_ready(push_ready_w),
    .pop_valid(pop_valid_w),
    .pop_ready(pop & ~(push & pop & empty)),
    .pop_data(pop_data_w)
  );
  // Inlined gate: `push_valid = push & ~feedthrough` written out so
  // there's a single continuous assign per inst input. See header.
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      data_out <= 0;
      empty <= 1'b1;
      full <= 1'b0;
    end else begin
      empty <= ~pop_valid_w;
      full <= ~push_ready_w;
      if (push & pop & empty) begin
        data_out <= data_in;
      end else if (pop & pop_valid_w) begin
        data_out <= pop_data_w;
      end
    end
  end

endmodule

