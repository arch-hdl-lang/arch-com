// CDC Pulse Synchronizer: toggle-FF synchronizer
// Supports NUM_CHANNELS channels
// Also provides rst_src_sync and rst_des_sync for reset synchronization
module cdc_pulse_synchronizer #(
  parameter int NUM_CHANNELS = 1
) (
  input logic src_clock,
  input logic des_clock,
  input logic rst_in,
  input logic [NUM_CHANNELS-1:0] src_pulse,
  output logic [NUM_CHANNELS-1:0] des_pulse,
  output logic rst_src_sync,
  output logic rst_des_sync
);

  // Toggle registers in src domain (one per channel)
  logic [NUM_CHANNELS-1:0] toggle_r;
  always_ff @(posedge src_clock or posedge rst_in) begin
    if (rst_in) begin
      toggle_r <= 0;
    end else begin
      for (int i = 0; i <= NUM_CHANNELS - 1; i++) begin
        if (src_pulse[i +: 1]) begin
          toggle_r[i +: 1] <= ~toggle_r[i +: 1];
        end
      end
    end
  end
  // 2-FF synchronizers for each channel (des_clock domain)
  logic [NUM_CHANNELS-1:0] sync1_r;
  logic [NUM_CHANNELS-1:0] sync2_r;
  logic [NUM_CHANNELS-1:0] prev_r;
  always_ff @(posedge des_clock or posedge rst_in) begin
    if (rst_in) begin
      prev_r <= 0;
      sync1_r <= 0;
      sync2_r <= 0;
    end else begin
      sync1_r <= toggle_r;
      sync2_r <= sync1_r;
      prev_r <= sync2_r;
    end
  end
  assign des_pulse = sync2_r ^ prev_r;
  // Reset synchronizers: 2FF sync of rst_in into each clock domain
  logic rst_src1;
  logic rst_src2;
  always_ff @(posedge src_clock or posedge rst_in) begin
    if (rst_in) begin
      rst_src1 <= 1'b1;
      rst_src2 <= 1'b1;
    end else begin
      rst_src1 <= 1'b0;
      rst_src2 <= rst_src1;
    end
  end
  assign rst_src_sync = rst_src2;
  logic rst_des1;
  logic rst_des2;
  always_ff @(posedge des_clock or posedge rst_in) begin
    if (rst_in) begin
      rst_des1 <= 1'b1;
      rst_des2 <= 1'b1;
    end else begin
      rst_des1 <= 1'b0;
      rst_des2 <= rst_des1;
    end
  end
  assign rst_des_sync = rst_des2;

endmodule

