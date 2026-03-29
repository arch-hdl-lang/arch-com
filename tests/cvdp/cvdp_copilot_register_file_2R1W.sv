module cvdp_copilot_register_file_2R1W #(
  parameter int DATA_WIDTH = 32
) (
  input logic [DATA_WIDTH-1:0] din,
  input logic [5-1:0] wad1,
  input logic [5-1:0] rad1,
  input logic [5-1:0] rad2,
  input logic wen1,
  input logic ren1,
  input logic ren2,
  input logic clk,
  input logic resetn,
  output logic [DATA_WIDTH-1:0] dout1,
  output logic [DATA_WIDTH-1:0] dout2,
  output logic collision
);

  // Clock gating: enable latch captured on falling edge
  logic en_latch_r = 1'b0;
  logic gated_clk_w;
  always_ff @(negedge clk) begin
    en_latch_r <= wen1 | ren1 | ren2;
  end
  assign gated_clk_w = en_latch_r & clk;
  // Register file memory
  logic [DATA_WIDTH-1:0] rf_mem [0:32-1];
  logic [32-1:0] rf_valid;
  // Write logic on gated clock
  always_ff @(posedge gated_clk_w or negedge resetn) begin
    if ((!resetn)) begin
      for (int __ri0 = 0; __ri0 < 32; __ri0++) begin
        rf_mem[__ri0] <= 0;
      end
      rf_valid <= 0;
    end else begin
      if (wen1) begin
        rf_mem[wad1] <= din;
        rf_valid <= rf_valid | 32'd1 << wad1;
      end
    end
  end
  // Read port 1 on gated clock
  always_ff @(posedge gated_clk_w or negedge resetn) begin
    if ((!resetn)) begin
      dout1 <= 0;
    end else begin
      if (ren1) begin
        if (rf_valid[rad1]) begin
          dout1 <= rf_mem[rad1];
        end else begin
          dout1 <= 0;
        end
      end else begin
        dout1 <= 0;
      end
    end
  end
  // Read port 2 on gated clock
  always_ff @(posedge gated_clk_w or negedge resetn) begin
    if ((!resetn)) begin
      dout2 <= 0;
    end else begin
      if (ren2) begin
        if (rf_valid[rad2]) begin
          dout2 <= rf_mem[rad2];
        end else begin
          dout2 <= 0;
        end
      end else begin
        dout2 <= 0;
      end
    end
  end
  // Collision detection on original clock
  always_ff @(posedge clk or negedge resetn) begin
    if ((!resetn)) begin
      collision <= 1'b0;
    end else begin
      if (ren1 & ren2 & rad1 == rad2 | wen1 & ren1 & wad1 == rad1 | wen1 & ren2 & wad1 == rad2) begin
        collision <= 1'b1;
      end else begin
        collision <= 1'b0;
      end
    end
  end

endmodule

