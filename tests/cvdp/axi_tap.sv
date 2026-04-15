module axi_tap #(
  parameter int ADDR_WIDTH = 32,
  parameter int DATA_WIDTH = 32
) (
  input logic clk_i,
  input logic rst_i,
  input logic inport_awvalid_i,
  input logic [ADDR_WIDTH-1:0] inport_awaddr_i,
  output logic inport_awready_o,
  input logic inport_wvalid_i,
  input logic [DATA_WIDTH-1:0] inport_wdata_i,
  input logic [3:0] inport_wstrb_i,
  output logic inport_wready_o,
  input logic inport_bready_i,
  output logic inport_bvalid_o,
  output logic [1:0] inport_bresp_o,
  input logic inport_arvalid_i,
  input logic [ADDR_WIDTH-1:0] inport_araddr_i,
  output logic inport_arready_o,
  input logic inport_rready_i,
  output logic inport_rvalid_o,
  output logic [DATA_WIDTH-1:0] inport_rdata_o,
  output logic [1:0] inport_rresp_o,
  input logic outport_awready_i,
  output logic outport_awvalid_o,
  output logic [ADDR_WIDTH-1:0] outport_awaddr_o,
  input logic outport_wready_i,
  output logic outport_wvalid_o,
  output logic [DATA_WIDTH-1:0] outport_wdata_o,
  output logic [3:0] outport_wstrb_o,
  input logic outport_bvalid_i,
  input logic [1:0] outport_bresp_i,
  output logic outport_bready_o,
  input logic outport_arready_i,
  output logic outport_arvalid_o,
  output logic [ADDR_WIDTH-1:0] outport_araddr_o,
  input logic outport_rvalid_i,
  input logic [DATA_WIDTH-1:0] outport_rdata_i,
  input logic [1:0] outport_rresp_i,
  output logic outport_rready_o,
  input logic outport_peripheral0_awready_i,
  output logic outport_peripheral0_awvalid_o,
  output logic [ADDR_WIDTH-1:0] outport_peripheral0_awaddr_o,
  input logic outport_peripheral0_wready_i,
  output logic outport_peripheral0_wvalid_o,
  output logic [DATA_WIDTH-1:0] outport_peripheral0_wdata_o,
  output logic [3:0] outport_peripheral0_wstrb_o,
  input logic [1:0] outport_peripheral0_bresp_i,
  input logic outport_peripheral0_bvalid_i,
  output logic outport_peripheral0_bready_o,
  input logic outport_peripheral0_arready_i,
  output logic outport_peripheral0_arvalid_o,
  output logic [ADDR_WIDTH-1:0] outport_peripheral0_araddr_o,
  input logic [1:0] outport_peripheral0_rresp_i,
  input logic outport_peripheral0_rvalid_i,
  input logic [DATA_WIDTH-1:0] outport_peripheral0_rdata_i,
  output logic outport_peripheral0_rready_o
);

  // Global ports
  // Master inport - Write Address Channel (AW)
  // Master inport - Write Data Channel (W)
  // Master inport - Write Response Channel (B)
  // Master inport - Read Address Channel (AR)
  // Master inport - Read Data Channel (R)
  // Default outport - Write Address Channel (AW)
  // Default outport - Write Data Channel (W)
  // Default outport - Write Response Channel (B)
  // Default outport - Read Address Channel (AR)
  // Default outport - Read Data Channel (R)
  // Peripheral0 outport - Write Address Channel (AW)
  // Peripheral0 outport - Write Data Channel (W)
  // Peripheral0 outport - Write Response Channel (B)
  // Peripheral0 outport - Read Address Channel (AR)
  // Peripheral0 outport - Read Data Channel (R)
  // Read tracking registers
  logic [3:0] read_pending_q;
  logic [0:0] read_port_q;
  // Write tracking registers
  logic [3:0] write_pending_q;
  logic [0:0] write_port_q;
  logic awvalid_q;
  logic wvalid_q;
  // Combinational wires
  logic [0:0] read_port_r;
  logic [3:0] read_pending_r;
  logic [0:0] write_port_r;
  logic [3:0] write_pending_r;
  logic read_accept_w;
  logic write_accept_w;
  logic wr_cmd_accepted_w;
  logic wr_data_accepted_w;
  logic outport_rvalid_r;
  logic [DATA_WIDTH-1:0] outport_rdata_r;
  logic [1:0] outport_rresp_r;
  logic outport_bvalid_r;
  logic [1:0] outport_bresp_r;
  logic inport_arready_r;
  logic inport_awready_r;
  logic inport_wready_r;
  //---------------------------------------------------------------
  // Read port selection: address decode
  //---------------------------------------------------------------
  always_comb begin
    if ((inport_araddr_i & 32'd2147483648) == 32'd2147483648) begin
      read_port_r = 1;
    end else begin
      read_port_r = 0;
    end
  end
  //---------------------------------------------------------------
  // Read pending counter
  //---------------------------------------------------------------
  always_comb begin
    read_pending_r = read_pending_q;
    if (inport_arvalid_i & inport_arready_o & ~(inport_rvalid_o & inport_rready_i)) begin
      read_pending_r = 4'(read_pending_q + 1);
    end else if (~(inport_arvalid_i & inport_arready_o) & inport_rvalid_o & inport_rready_i) begin
      read_pending_r = 4'(read_pending_q - 1);
    end
  end
  //---------------------------------------------------------------
  // Read registers
  //---------------------------------------------------------------
  always_ff @(posedge clk_i) begin
    if (rst_i) begin
      read_pending_q <= 0;
      read_port_q <= 0;
    end else begin
      read_pending_q <= read_pending_r;
      if (inport_arvalid_i & inport_arready_o) begin
        read_port_q <= read_port_r;
      end
    end
  end
  //---------------------------------------------------------------
  // Read accept
  //---------------------------------------------------------------
  assign read_accept_w = ((read_port_q == read_port_r) & (read_pending_q != 4'd15)) | (read_pending_q == 0);
  //---------------------------------------------------------------
  // Read channel: outport default
  //---------------------------------------------------------------
  assign outport_arvalid_o = inport_arvalid_i & read_accept_w & (read_port_r == 0);
  assign outport_araddr_o = inport_araddr_i;
  assign outport_rready_o = inport_rready_i;
  //---------------------------------------------------------------
  // Read channel: peripheral0
  //---------------------------------------------------------------
  assign outport_peripheral0_arvalid_o = inport_arvalid_i & read_accept_w & (read_port_r == 1);
  assign outport_peripheral0_araddr_o = inport_araddr_i;
  assign outport_peripheral0_rready_o = inport_rready_i;
  //---------------------------------------------------------------
  // Read response mux
  //---------------------------------------------------------------
  always_comb begin
    if (read_port_q == 1) begin
      outport_rvalid_r = outport_peripheral0_rvalid_i;
      outport_rdata_r = outport_peripheral0_rdata_i;
      outport_rresp_r = outport_peripheral0_rresp_i;
    end else begin
      outport_rvalid_r = outport_rvalid_i;
      outport_rdata_r = outport_rdata_i;
      outport_rresp_r = outport_rresp_i;
    end
  end
  assign inport_rvalid_o = outport_rvalid_r;
  assign inport_rdata_o = outport_rdata_r;
  assign inport_rresp_o = outport_rresp_r;
  //---------------------------------------------------------------
  // Read arready mux
  //---------------------------------------------------------------
  always_comb begin
    if (read_port_r == 1) begin
      inport_arready_r = outport_peripheral0_arready_i;
    end else begin
      inport_arready_r = outport_arready_i;
    end
  end
  assign inport_arready_o = read_accept_w & inport_arready_r;
  //---------------------------------------------------------------
  // Write command/data tracking
  //---------------------------------------------------------------
  assign wr_cmd_accepted_w = (inport_awvalid_i & inport_awready_o) | awvalid_q;
  assign wr_data_accepted_w = (inport_wvalid_i & inport_wready_o) | wvalid_q;
  always_ff @(posedge clk_i) begin
    if (rst_i) begin
      awvalid_q <= 1'b0;
    end else begin
      if (inport_awvalid_i & inport_awready_o & ~wr_data_accepted_w) begin
        awvalid_q <= 1'b1;
      end else if (wr_data_accepted_w) begin
        awvalid_q <= 1'b0;
      end
    end
  end
  always_ff @(posedge clk_i) begin
    if (rst_i) begin
      wvalid_q <= 1'b0;
    end else begin
      if (inport_wvalid_i & inport_wready_o & ~wr_cmd_accepted_w) begin
        wvalid_q <= 1'b1;
      end else if (wr_cmd_accepted_w) begin
        wvalid_q <= 1'b0;
      end
    end
  end
  //---------------------------------------------------------------
  // Write port selection: address decode
  //---------------------------------------------------------------
  always_comb begin
    if ((inport_awaddr_i & 32'd2147483648) == 32'd2147483648) begin
      write_port_r = 1;
    end else begin
      write_port_r = 0;
    end
  end
  //---------------------------------------------------------------
  // Write pending counter
  //---------------------------------------------------------------
  always_comb begin
    write_pending_r = write_pending_q;
    if (wr_cmd_accepted_w & wr_data_accepted_w & ~(inport_bvalid_o & inport_bready_i)) begin
      write_pending_r = 4'(write_pending_q + 1);
    end else if (~(wr_cmd_accepted_w & wr_data_accepted_w) & inport_bvalid_o & inport_bready_i) begin
      write_pending_r = 4'(write_pending_q - 1);
    end
  end
  //---------------------------------------------------------------
  // Write registers
  //---------------------------------------------------------------
  always_ff @(posedge clk_i) begin
    if (rst_i) begin
      write_pending_q <= 0;
      write_port_q <= 0;
    end else begin
      write_pending_q <= write_pending_r;
      if (inport_awvalid_i & inport_awready_o) begin
        write_port_q <= write_port_r;
      end
    end
  end
  //---------------------------------------------------------------
  // Write accept
  //---------------------------------------------------------------
  assign write_accept_w = ((write_port_q == write_port_r) & (write_pending_q != 4'd15)) | (write_pending_q == 0);
  //---------------------------------------------------------------
  // Write channel: outport default
  //---------------------------------------------------------------
  assign outport_awvalid_o = inport_awvalid_i & ~awvalid_q & write_accept_w & (write_port_r == 0);
  assign outport_awaddr_o = inport_awaddr_i;
  assign outport_wvalid_o = inport_wvalid_i & ~wvalid_q & (inport_awvalid_i | awvalid_q) & (write_port_r == 0);
  assign outport_wdata_o = inport_wdata_i;
  assign outport_wstrb_o = inport_wstrb_i;
  assign outport_bready_o = inport_bready_i;
  //---------------------------------------------------------------
  // Write channel: peripheral0
  //---------------------------------------------------------------
  assign outport_peripheral0_awvalid_o = inport_awvalid_i & ~awvalid_q & write_accept_w & (write_port_r == 1);
  assign outport_peripheral0_awaddr_o = inport_awaddr_i;
  assign outport_peripheral0_wvalid_o = inport_wvalid_i & ~wvalid_q & ((inport_awvalid_i & write_accept_w) | awvalid_q) & (write_port_r == 1);
  assign outport_peripheral0_wdata_o = inport_wdata_i;
  assign outport_peripheral0_wstrb_o = inport_wstrb_i;
  assign outport_peripheral0_bready_o = inport_bready_i;
  //---------------------------------------------------------------
  // Write response mux
  //---------------------------------------------------------------
  always_comb begin
    if (write_port_q == 1) begin
      outport_bvalid_r = outport_peripheral0_bvalid_i;
      outport_bresp_r = outport_peripheral0_bresp_i;
    end else begin
      outport_bvalid_r = outport_bvalid_i;
      outport_bresp_r = outport_bresp_i;
    end
  end
  assign inport_bvalid_o = outport_bvalid_r;
  assign inport_bresp_o = outport_bresp_r;
  //---------------------------------------------------------------
  // Write awready/wready mux
  //---------------------------------------------------------------
  always_comb begin
    if (write_port_r == 1) begin
      inport_awready_r = outport_peripheral0_awready_i;
      inport_wready_r = outport_peripheral0_wready_i;
    end else begin
      inport_awready_r = outport_awready_i;
      inport_wready_r = outport_wready_i;
    end
  end
  assign inport_awready_o = write_accept_w & ~awvalid_q & inport_awready_r;
  assign inport_wready_o = write_accept_w & ~wvalid_q & inport_wready_r;

endmodule

