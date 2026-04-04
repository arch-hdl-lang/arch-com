module secure_read_write_bus_interface #(
  parameter int P_CONFIGURABLE_KEY = 170,
  parameter int P_DATA_WIDTH = 8,
  parameter int P_ADDR_WIDTH = 8
) (
  input logic i_capture_pulse,
  input logic i_reset_bar,
  input logic [P_ADDR_WIDTH-1:0] i_addr,
  input logic [P_DATA_WIDTH-1:0] i_data_in,
  input logic [8-1:0] i_key_in,
  input logic i_read_write_enable,
  output logic [P_DATA_WIDTH-1:0] o_data_out,
  output logic o_error
);

  logic [P_DATA_WIDTH-1:0] mem [256-1:0];
  always_ff @(posedge i_capture_pulse or negedge i_reset_bar) begin
    if ((!i_reset_bar)) begin
      for (int __ri0 = 0; __ri0 < 256; __ri0++) begin
        mem[__ri0] <= 0;
      end
      o_data_out <= 0;
      o_error <= 0;
    end else begin
      if (i_key_in == P_CONFIGURABLE_KEY) begin
        o_error <= 0;
        if (i_read_write_enable == 0) begin
          mem[i_addr] <= i_data_in;
          o_data_out <= 0;
        end else begin
          o_data_out <= mem[i_addr];
        end
      end else begin
        o_error <= 1;
        o_data_out <= 0;
      end
    end
  end

endmodule

