module sync_muller_c_element #(
  parameter int NUM_INPUT = 2,
  parameter int PIPE_DEPTH = 1
) (
  input logic clk,
  input logic srst,
  input logic [1-1:0] clr,
  input logic [1-1:0] clk_en,
  input logic [NUM_INPUT-1:0] inp,
  output logic [1-1:0] out
);

  logic [NUM_INPUT-1:0] pipe [0:PIPE_DEPTH-1];
  logic [1-1:0] out_r;
  logic all_ones;
  logic all_zeros;
  assign all_ones = &pipe[PIPE_DEPTH - 1];
  assign all_zeros = ~|pipe[PIPE_DEPTH - 1];
  always_ff @(posedge clk) begin
    if (srst) begin
      out_r <= 0;
      for (int __ri0 = 0; __ri0 < PIPE_DEPTH; __ri0++) begin
        pipe[__ri0] <= 0;
      end
    end else begin
      if (clr) begin
        for (int i = 0; i <= PIPE_DEPTH - 1; i++) begin
          pipe[i] <= 0;
        end
        out_r <= 0;
      end else begin
        if (clk_en) begin
          pipe[0] <= inp;
          for (int i = 1; i <= PIPE_DEPTH - 1; i++) begin
            pipe[i] <= pipe[i - 1];
          end
        end
        if (clk_en) begin
          if (all_ones) begin
            out_r <= 1;
          end else if (all_zeros) begin
            out_r <= 0;
          end
        end
      end
    end
  end
  assign out = out_r;

endmodule

