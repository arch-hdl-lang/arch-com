module adc_data_rotate #(
  parameter int DATA_WIDTH = 8
) (
  input logic i_clk,
  input logic i_rst_n,
  input logic [DATA_WIDTH-1:0] i_adc_data_in,
  input logic [3:0] i_shift_count,
  input logic i_shift_direction,
  output logic [DATA_WIDTH-1:0] o_processed_data,
  output logic o_operation_status
);

  // Modulo DATA_WIDTH for shift amount
  logic [3:0] mod_shift;
  assign mod_shift = 4'(32'($unsigned(i_shift_count)) % DATA_WIDTH);
  logic [3:0] rev_shift;
  assign rev_shift = 4'(DATA_WIDTH - 32'($unsigned(mod_shift)));
  always_ff @(posedge i_clk or negedge i_rst_n) begin
    if ((!i_rst_n)) begin
      o_operation_status <= 1'b0;
      o_processed_data <= 0;
    end else begin
      o_operation_status <= 1'b1;
      if (i_shift_direction) begin
        // Right rotate
        o_processed_data <= i_adc_data_in >> mod_shift | i_adc_data_in << rev_shift;
      end else begin
        // Left rotate
        o_processed_data <= i_adc_data_in << mod_shift | i_adc_data_in >> rev_shift;
      end
    end
  end

endmodule

