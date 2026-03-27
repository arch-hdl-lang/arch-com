module serial_in_parallel_out_8bit (
  input logic clock,
  input logic reset,
  input logic serial_in,
  output logic [8-1:0] parallel_out
);

  always_ff @(posedge clock or posedge reset) begin
    if (reset) begin
      parallel_out <= 0;
    end else begin
      parallel_out <= {parallel_out[6:0], serial_in};
    end
  end

endmodule

