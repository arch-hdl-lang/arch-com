module serial_in_parallel_out_8bit (
  input logic clock,
  input logic serial_in,
  output logic [7:0] parallel_out
);

  logic [7:0] data = 0;
  logic [7:0] shifted;
  assign shifted = data << 1 | 8'($unsigned(serial_in));
  assign parallel_out = shifted;
  always_ff @(negedge clock) begin
    data <= shifted;
  end

endmodule

