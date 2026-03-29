module cvdp_copilot_apb_gpio #(
  parameter int GPIO_WIDTH = 8
) (
  input logic pclk,
  input logic preset_n,
  input logic psel,
  input logic [6-1:0] paddr,
  input logic penable,
  input logic pwrite,
  input logic [32-1:0] pwdata,
  input logic [GPIO_WIDTH-1:0] gpio_in,
  output logic [32-1:0] prdata,
  output logic pready,
  output logic pslverr,
  output logic [GPIO_WIDTH-1:0] gpio_out,
  output logic [GPIO_WIDTH-1:0] gpio_enable,
  output logic [GPIO_WIDTH-1:0] gpio_int,
  output logic comb_int
);

  // Internal registers
  logic [GPIO_WIDTH-1:0] reg_dout;
  logic [GPIO_WIDTH-1:0] reg_dout_en;
  logic [GPIO_WIDTH-1:0] reg_int_en;
  logic [GPIO_WIDTH-1:0] reg_int_type;
  logic [GPIO_WIDTH-1:0] reg_int_pol;
  logic [GPIO_WIDTH-1:0] reg_int_state;
  // Two-stage synchronizer for gpio_in
  logic [GPIO_WIDTH-1:0] sync1;
  logic [GPIO_WIDTH-1:0] sync2;
  logic [GPIO_WIDTH-1:0] sync_prev;
  always_ff @(posedge pclk or negedge preset_n) begin
    if ((!preset_n)) begin
      sync1 <= 0;
      sync2 <= 0;
      sync_prev <= 0;
    end else begin
      sync1 <= gpio_in;
      sync2 <= sync1;
      sync_prev <= sync2;
    end
  end
  // Wires for interrupt logic
  logic [GPIO_WIDTH-1:0] rising_edge;
  logic [GPIO_WIDTH-1:0] falling_edge;
  logic [GPIO_WIDTH-1:0] edge_detect;
  logic [GPIO_WIDTH-1:0] level_detect;
  logic [GPIO_WIDTH-1:0] int_combined;
  assign rising_edge = sync2 & ~sync_prev;
  assign falling_edge = ~sync2 & sync_prev;
  assign edge_detect = ~reg_int_pol & rising_edge | reg_int_pol & falling_edge;
  assign level_detect = sync2 ^ reg_int_pol;
  assign int_combined = reg_int_type & reg_int_state | ~reg_int_type & level_detect & reg_int_en;
  // Combinational outputs
  assign gpio_out = reg_dout;
  assign gpio_enable = reg_dout_en;
  assign pready = 1'b1;
  assign pslverr = 1'b0;
  assign gpio_int = int_combined;
  assign comb_int = gpio_int != 0;
  // APB control signals
  logic apb_write_en;
  assign apb_write_en = psel & penable & pwrite;
  logic apb_read_en;
  assign apb_read_en = psel & penable & ~pwrite;
  // APB write logic + edge interrupt accumulation
  always_ff @(posedge pclk or negedge preset_n) begin
    if ((!preset_n)) begin
      reg_dout <= 0;
      reg_dout_en <= 0;
      reg_int_en <= 0;
      reg_int_pol <= 0;
      reg_int_state <= 0;
      reg_int_type <= 0;
    end else begin
      reg_int_state <= (reg_int_state | edge_detect & reg_int_en) & reg_int_type;
      if (apb_write_en) begin
        if (paddr == 1) begin
          reg_dout <= pwdata[GPIO_WIDTH - 1:0];
        end else if (paddr == 2) begin
          reg_dout_en <= pwdata[GPIO_WIDTH - 1:0];
        end else if (paddr == 3) begin
          reg_int_en <= pwdata[GPIO_WIDTH - 1:0];
        end else if (paddr == 4) begin
          reg_int_type <= pwdata[GPIO_WIDTH - 1:0];
        end else if (paddr == 5) begin
          reg_int_pol <= pwdata[GPIO_WIDTH - 1:0];
        end else if (paddr == 6) begin
          reg_int_state <= (reg_int_state | edge_detect & reg_int_en) & reg_int_type & ~(pwdata[GPIO_WIDTH - 1:0] & reg_int_type);
        end
      end
    end
  end
  // Edge interrupt accumulation: accumulate edges, keep only edge-type bits
  // Write-1-to-clear for edge interrupts
  // APB read logic (combinational)
  always_comb begin
    if (apb_read_en) begin
      if (paddr == 0) begin
        prdata = 32'($unsigned(sync2));
      end else if (paddr == 1) begin
        prdata = 32'($unsigned(reg_dout));
      end else if (paddr == 2) begin
        prdata = 32'($unsigned(reg_dout_en));
      end else if (paddr == 3) begin
        prdata = 32'($unsigned(reg_int_en));
      end else if (paddr == 4) begin
        prdata = 32'($unsigned(reg_int_type));
      end else if (paddr == 5) begin
        prdata = 32'($unsigned(reg_int_pol));
      end else if (paddr == 6) begin
        prdata = 32'($unsigned(int_combined));
      end else begin
        prdata = 0;
      end
    end else begin
      prdata = 0;
    end
  end

endmodule

