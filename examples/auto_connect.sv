//! ---
//! tags: [inst, auto-connect, hierarchy, tutorial]
//! ---
//!
//! `auto;` — auto-connect by name inside an `inst` body.
//!
//! A single `auto;` line wires every child port the explicit connections
//! left unconnected to the identically-named signal in scope. Explicit
//! connections always win, wherever they sit relative to the directive.
//!
//! This is the shape `auto;` exists for: two sub-instances whose `clk`,
//! `rst` and `en` plumbing is pure boilerplate, and exactly one connection
//! per instance that actually carries design intent (`din <- ...`). Without
//! the directive, `Top` would spell out 8 connections instead of 2.
//!
//! `auto;` is a front-end desugar — the emitted SystemVerilog is identical
//! to writing every connection by hand. Run `arch build --explain-auto` to
//! print what each directive expanded to.
/// 100 MHz system clock domain (single-clock design).
// domain SysDomain
//   freq_mhz: 100

/// Running 8-bit accumulator: `sum += din` while `en` is high.
module Accum (
  input logic clk,
  input logic rst,
  input logic en,
  input logic [7:0] din,
  output logic [7:0] sum
);

  logic [7:0] acc;
  always_ff @(posedge clk) begin
    if (rst) begin
      acc <= 0;
    end else begin
      if (en) begin
        acc <= 8'(acc + din);
      end
    end
  end
  assign sum = acc;

endmodule

/// Registers `din` doubled (wrapping), one cycle behind, while `en` is high.
module Doubler (
  input logic clk,
  input logic rst,
  input logic en,
  input logic [7:0] din,
  output logic [7:0] dbl
);

  logic [7:0] r;
  always_ff @(posedge clk) begin
    if (rst) begin
      r <= 0;
    end else begin
      if (en) begin
        r <= 8'(din + din);
      end
    end
  end
  assign dbl = r;

endmodule

/// Feeds one input to both leaves and exposes both results.
///
/// Every connection below is either name-identical (filled by `auto;`) or
/// written out because the names genuinely differ.
module AutoConnectTop (
  input logic clk,
  input logic rst,
  input logic en,
  input logic [7:0] sample,
  output logic [7:0] sum,
  output logic [7:0] dbl
);

  // `din` differs from the parent's `sample`, so it stays explicit; `clk`,
  // `rst`, `en` and `sum` are name-identical and `auto;` fills them.
  Accum acc0 (
    .din(sample),
    .clk(clk),
    .rst(rst),
    .en(en),
    .sum(sum)
  );
  // Same shape for the second leaf — the directive is order-independent, so
  // it can sit before the explicit line just as well.
  Doubler dbl0 (
    .din(sample),
    .clk(clk),
    .rst(rst),
    .en(en),
    .dbl(dbl)
  );

endmodule

