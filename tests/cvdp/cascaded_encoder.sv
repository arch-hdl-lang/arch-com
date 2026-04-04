module cascaded_encoder #(
  parameter int N = 8,
  parameter int M = 3
) (
  input logic [N-1:0] input_signal,
  output logic [M-1:0] out,
  output logic [2-1:0] out_upper_half,
  output logic [2-1:0] out_lower_half
);

  logic [2-1:0] upper_out;
  logic [2-1:0] lower_out;
  logic upper_active;
  priority_encoder #(.N(4), .M(2)) upper_enc (
    .input_signal(input_signal[7:4]),
    .out(upper_out)
  );
  priority_encoder #(.N(4), .M(2)) lower_enc (
    .input_signal(input_signal[3:0]),
    .out(lower_out)
  );
  assign out_upper_half = upper_out;
  assign out_lower_half = lower_out;
  assign upper_active = input_signal[7:4] != 0;
  always_comb begin
    if (upper_active) begin
      out = {1'b1, upper_out};
    end else begin
      out = {1'b0, lower_out};
    end
  end

endmodule

