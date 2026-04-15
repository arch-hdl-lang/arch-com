module single_neuron_dut (
  input logic clk,
  input logic rst_n,
  input logic ctrl,
  input logic [7:0] seq_in,
  output logic [7:0] seq_out
);

  logic [7:0] state_r;
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

