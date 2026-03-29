module register (
  input logic clk,
  input logic rst,
  input logic [4-1:0] data_in,
  input logic valid_in,
  input logic ready_in,
  output logic [4-1:0] data_out,
  output logic valid_out,
  output logic ready_out
);

  logic [4-1:0] mem;
  logic data_present;
  assign ready_out = ~data_present | ready_in;
  assign valid_out = data_present;
  assign data_out = mem;
  always_ff @(posedge clk) begin
    if (rst) begin
      data_present <= 1'b0;
      mem <= 0;
    end else begin
      if (rst) begin
        mem <= 0;
        data_present <= 1'b0;
      end else if (data_present) begin
        if (ready_in) begin
          if (valid_in) begin
            mem <= data_in;
            data_present <= 1'b1;
          end else begin
            mem <= 0;
            data_present <= 1'b0;
          end
        end
      end else if (valid_in) begin
        mem <= data_in;
        data_present <= 1'b1;
      end
    end
  end

endmodule

