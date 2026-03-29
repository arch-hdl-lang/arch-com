module neuromorphic_array #(
  parameter int NEURONS = 8,
  parameter int INPUTS = 8,
  parameter int OUTPUTS = 8
) (
  input logic [8-1:0] ui_in,
  input logic [8-1:0] uio_in,
  output logic [8-1:0] uo_out,
  input logic clk,
  input logic rst_n
);

  logic [8-1:0] neuron_outputs [0:NEURONS-1];
  genvar i;
  for (i = 0; i <= NEURONS - 1; i = i + 1) begin : gen_i
    single_neuron_dut neuron_i (
      .clk(clk),
      .rst_n(rst_n),
      .ctrl(ui_in[0]),
      .seq_in(uio_in),
      .seq_out(neuron_outputs[i])
    );
  end
  assign uo_out = neuron_outputs[NEURONS - 1];

endmodule

module single_neuron_dut (
  input logic clk,
  input logic rst_n,
  input logic ctrl,
  input logic [8-1:0] seq_in,
  output logic [8-1:0] seq_out
);

  logic [8-1:0] state_r;
  always_ff @(posedge clk) begin
    if ((!rst_n)) begin
      state_r <= 0;
    end else begin
      if (ctrl) begin
        state_r <= seq_in;
      end
    end
  end
  assign seq_out = state_r;

endmodule

