module neuromorphic_array #(
  parameter int NEURONS = 8,
  parameter int INPUTS = 8,
  parameter int OUTPUTS = 8
) (
  input logic [7:0] ui_in,
  input logic [7:0] uio_in,
  output logic [7:0] uo_out,
  input logic clk,
  input logic rst_n
);

  logic [NEURONS-1:0] [7:0] neuron_outputs;
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

