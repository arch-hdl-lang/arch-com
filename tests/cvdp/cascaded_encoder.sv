module cascaded_encoder #(
  parameter int N = 8,
  parameter int M = 3,
  parameter int HALF = N / 2
) (
  input logic [N-1:0] input_signal,
  output logic [M-1:0] out,
  output logic [M - 1-1:0] out_upper_half,
  output logic [M - 1-1:0] out_lower_half
);

  logic [M - 1-1:0] upper_out;
  logic [M - 1-1:0] lower_out;
  logic upper_active;
  priority_encoder #(.N(HALF), .M(M - 1)) upper_enc (
    .input_signal(input_signal[N - 1:HALF]),
    .out(upper_out)
  );
  priority_encoder #(.N(HALF), .M(M - 1)) lower_enc (
    .input_signal(input_signal[HALF - 1:0]),
    .out(lower_out)
  );
  assign out_upper_half = upper_out;
  assign out_lower_half = lower_out;
  assign upper_active = input_signal[N - 1:HALF] != 0;
  always_comb begin
    if (upper_active) begin
      out = {1'b1, upper_out};
    end else begin
      out = {1'b0, lower_out};
    end
  end

endmodule

module priority_encoder #(
  parameter int N = 8,
  parameter int M = 3
) (
  input logic [N-1:0] input_signal,
  output logic [M-1:0] out
);

  logic [M-1:0] result;
  always_comb begin
    result = 0;
    for (int i = 0; i <= N - 1; i++) begin
      if (input_signal[i +: 1]) begin
        result = i[M - 1:0];
      end
    end
  end
  assign out = result;

endmodule

