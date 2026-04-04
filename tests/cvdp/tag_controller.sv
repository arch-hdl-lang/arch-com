module tag_controller (
  input logic clk,
  input logic rst,
  input logic write_enable,
  input logic [9-1:0] write_addr,
  input logic [8-1:0] read_addr_0,
  input logic [8-1:0] read_addr_1,
  input logic [8-1:0] data_in_0,
  input logic [8-1:0] data_in_1,
  output logic [9-1:0] data_out_0,
  output logic [9-1:0] data_out_1,
  output logic write_enable_0,
  output logic write_enable_1,
  output logic [8-1:0] addr_out_0,
  output logic [8-1:0] addr_out_1
);

  logic [256-1:0] ram_store;
  logic [8-1:0] tag_addr_0;
  assign tag_addr_0 = write_enable ? write_addr[7:0] : read_addr_0;
  logic [8-1:0] tag_addr_1;
  assign tag_addr_1 = write_enable ? write_addr[7:0] : read_addr_1;
  always_ff @(posedge clk or posedge rst) begin
    if (rst) begin
      ram_store <= 0;
    end else begin
      if (write_enable) begin
        ram_store[write_addr[7:0]] <= 1;
      end
    end
  end
  assign addr_out_0 = tag_addr_0;
  assign addr_out_1 = tag_addr_1;
  assign write_enable_0 = write_enable;
  assign write_enable_1 = write_enable;
  assign data_out_0 = {ram_store[read_addr_0], data_in_0};
  assign data_out_1 = {ram_store[read_addr_1], data_in_1};

endmodule

