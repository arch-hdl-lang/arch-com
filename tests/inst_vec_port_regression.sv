// Regression test for the sub-instance Vec port wiring fix.
//
// The sub-module declares an output port whose Vec count is a param.
// The parent instantiates it with `param N = 8;`. Pre-fix, the
// parent's sim_codegen looked up the sub-module's port type and
// asked vec_array_info for the count, which fell through to 0
// because N is an Ident. The Vec wiring was then silently dropped
// (count==0 filter), and the parent read garbage from the sub's
// outputs.
//
// Post-fix: lookup_inst_params + inst-time param overrides feed
// vec_array_info_with_params, so the count resolves correctly and
// the per-element wiring is emitted.
module Mirror #(
  parameter int N = 8
) (
  input logic clk,
  input logic rst,
  input logic [N-1:0] [7:0] in_vec,
  output logic [N-1:0] [7:0] out_vec
);

  always_comb begin
    for (int i = 0; i <= N - 1; i++) begin
      out_vec[i] = in_vec[i];
    end
  end

endmodule

module Top (
  input logic clk,
  input logic rst,
  input logic [7:0] [7:0] src,
  output logic [7:0] [7:0] dst
);

  Mirror #(.N(8)) mirror (
    .clk(clk),
    .rst(rst),
    .in_vec(src),
    .out_vec(dst)
  );

endmodule

