module run_length #(
  parameter int DATA_WIDTH = 8
) (
  input logic clk,
  input logic reset_n,
  input logic data_in,
  output logic data_out,
  output logic [$clog2(DATA_WIDTH) + 1-1:0] run_value,
  output logic valid
);

  logic [$clog2(DATA_WIDTH) + 1-1:0] run_len = 0;
  logic prev_data_in = 0;
  always_ff @(posedge clk or negedge reset_n) begin
    if ((!reset_n)) begin
      data_out <= 0;
      prev_data_in <= 0;
      run_len <= 0;
      run_value <= 0;
      valid <= 0;
    end else begin
      prev_data_in <= data_in;
      if (data_in == prev_data_in) begin
        if (32'($unsigned(run_len)) == DATA_WIDTH) begin
          run_value <= run_len;
          run_len <= 1;
          valid <= 1;
          data_out <= prev_data_in;
        end else begin
          run_len <= ($clog2(DATA_WIDTH) + 1)'(run_len + 1);
          valid <= 0;
        end
      end else begin
        run_value <= run_len;
        data_out <= prev_data_in;
        valid <= 1;
        run_len <= 1;
      end
    end
  end

endmodule

