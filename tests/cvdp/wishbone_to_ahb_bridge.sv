module wishbone_to_ahb_bridge (
  input logic clk_i,
  input logic rst_i,
  input logic cyc_i,
  input logic stb_i,
  input logic [3:0] sel_i,
  input logic we_i,
  input logic [31:0] addr_i,
  input logic [31:0] data_i,
  input logic hclk,
  input logic hreset_n,
  input logic [31:0] hrdata,
  input logic [1:0] hresp,
  input logic hready,
  output logic [31:0] data_o,
  output logic ack_o,
  output logic [1:0] htrans,
  output logic [2:0] hsize,
  output logic [2:0] hburst,
  output logic hwrite,
  output logic [31:0] haddr,
  output logic [31:0] hwdata
);

  // Wishbone ports
  // AHB ports
  // Wishbone outputs
  // AHB outputs
  // Active when both cyc and stb are asserted
  logic wb_active;
  assign wb_active = cyc_i & stb_i;
  // Determine hsize from sel_i
  logic [2:0] size_val;
  always_comb begin
    if (sel_i == 'b1111) begin
      size_val = 3'($unsigned('b10));
    end else if ((sel_i == 'b11) | (sel_i == 'b1100)) begin
      size_val = 3'($unsigned('b1));
    end else begin
      size_val = 3'($unsigned('b0));
    end
  end
  // AHB outputs - directly driven from WB inputs
  assign hwrite = we_i;
  assign haddr = addr_i;
  assign hwdata = data_i;
  assign hburst = 3'($unsigned('b0));
  assign hsize = size_val;
  // htrans: NONSEQ (2'b10) when active, IDLE (2'b00) otherwise
  always_comb begin
    if (wb_active) begin
      htrans = 'b10;
    end else begin
      htrans = 'b0;
    end
  end
  // data_o: pass through hrdata
  assign data_o = hrdata;
  // ack_o: acknowledge when active and hready
  assign ack_o = wb_active & hready;

endmodule

