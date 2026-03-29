module ahb_clock_counter #(
  parameter int ADDR_WIDTH = 32,
  parameter int DATA_WIDTH = 32
) (
  input logic HCLK,
  input logic HRESETn,
  input logic HSEL,
  input logic [ADDR_WIDTH-1:0] HADDR,
  input logic HWRITE,
  input logic [DATA_WIDTH-1:0] HWDATA,
  input logic HREADY,
  output logic [DATA_WIDTH-1:0] HRDATA,
  output logic HRESP,
  output logic [DATA_WIDTH-1:0] COUNTER
);

  logic [DATA_WIDTH-1:0] cnt;
  logic enable;
  logic overflow;
  logic [DATA_WIDTH-1:0] max_cnt;
  always_ff @(posedge HCLK or negedge HRESETn) begin
    if ((!HRESETn)) begin
      cnt <= 0;
      enable <= 0;
      max_cnt <= 0;
      overflow <= 0;
    end else begin
      // Write logic
      if (HSEL & HWRITE & HREADY) begin
        if (HADDR == 'h0) begin
          enable <= HWDATA[0:0];
        end else if (HADDR == 'h4) begin
          if (HWDATA[0:0]) begin
            enable <= 0;
          end
        end else if (HADDR == 'h10) begin
          max_cnt <= HWDATA;
        end
      end
      // Counter always counts when enabled
      if (enable) begin
        cnt <= DATA_WIDTH'(cnt + 1);
      end
      // Overflow flag (sticky until reset)
      if (enable & ~overflow) begin
        if (DATA_WIDTH'(cnt + 1) == max_cnt) begin
          overflow <= 1;
        end
      end
    end
  end
  logic [DATA_WIDTH-1:0] hrdata_val;
  assign hrdata_val = HADDR == 'h8 ? cnt : HADDR == 'hC ? DATA_WIDTH'($unsigned(overflow)) : HADDR == 'h10 ? max_cnt : HADDR == 'h0 ? DATA_WIDTH'($unsigned(enable)) : 0;
  assign HRDATA = hrdata_val;
  assign COUNTER = cnt;
  assign HRESP = 0;

endmodule

