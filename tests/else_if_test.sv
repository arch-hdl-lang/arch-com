// domain SysDomain
//   freq_mhz: 100

module ElseIfTest (
  input logic clk,
  input logic rst,
  input logic [2-1:0] sel,
  input logic [8-1:0] data_in,
  output logic [8-1:0] result,
  output logic [8-1:0] comb_out
);

  logic [8-1:0] result_r = 0;
  // Test else-if in seq block
  always_ff @(posedge clk) begin
    if (rst) begin
      result_r <= 0;
    end else begin
      if ((sel == 0)) begin
        result_r <= data_in;
      end else if ((sel == 1)) begin
        result_r <= (data_in ^ 'hFF);
      end else if ((sel == 2)) begin
        result_r <= 8'((data_in << 1));
      end else begin
        result_r <= 0;
      end
    end
  end
  // Test else-if in comb block
  always_comb begin
    if ((sel == 0)) begin
      comb_out = data_in;
    end else if ((sel == 1)) begin
      comb_out = (data_in ^ 'hFF);
    end else if ((sel == 2)) begin
      comb_out = 8'((data_in << 1));
    end else begin
      comb_out = 0;
    end
    result = result_r;
  end

endmodule

