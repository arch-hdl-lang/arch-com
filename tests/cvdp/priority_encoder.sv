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

