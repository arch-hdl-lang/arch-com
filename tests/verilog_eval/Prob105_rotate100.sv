// domain SysDomain

module TopModule (
  input logic clk,
  input logic load,
  input logic [2-1:0] ena,
  input logic [100-1:0] data,
  output logic [100-1:0] q
);

  logic [100-1:0] q_r;
  always_ff @(posedge clk) begin
    if (load) begin
      q_r <= data;
    end else if ((ena == 1)) begin
      q_r[99] <= q_r[0];
      for (int i = 0; i <= 98; i++) begin
        q_r[i] <= q_r[(i + 1)];
      end
    end else if ((ena == 2)) begin
      q_r[0] <= q_r[99];
      for (int i = 1; i <= 99; i++) begin
        q_r[i] <= q_r[(i - 1)];
      end
    end
  end
  assign q = q_r;

endmodule

