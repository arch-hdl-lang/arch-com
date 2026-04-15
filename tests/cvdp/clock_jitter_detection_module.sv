module clock_jitter_detection_module #(
  parameter int JITTER_THRESHOLD = 5
) (
  input logic clk,
  input logic system_clk,
  input logic rst,
  output logic jitter_detected
);

  logic [31:0] edge_count;
  logic [31:0] edge_count_r;
  logic prev_system_clk;
  logic edge_detected;
  logic start_counter;
  logic [31:0] cycle_count;
  logic cnt_ne_thresh;
  assign cnt_ne_thresh = edge_count != JITTER_THRESHOLD;
  logic cnt_ne_zero;
  assign cnt_ne_zero = edge_count != 0;
  logic past_startup;
  assign past_startup = cycle_count > JITTER_THRESHOLD;
  always_ff @(posedge clk) begin
    if (rst) begin
      cycle_count <= 0;
      edge_count <= 0;
      edge_count_r <= 0;
      edge_detected <= 1'b0;
      jitter_detected <= 1'b0;
      prev_system_clk <= 1'b0;
      start_counter <= 1'b0;
    end else begin
      prev_system_clk <= system_clk;
      edge_detected <= system_clk & ~prev_system_clk;
      cycle_count <= 32'(cycle_count + 1);
      if (edge_detected) begin
        edge_count_r <= edge_count;
        edge_count <= 1;
        start_counter <= 1'b1;
      end else if (start_counter) begin
        edge_count <= 32'(edge_count + 1);
      end
      if (edge_detected) begin
        if (cnt_ne_thresh & cnt_ne_zero & past_startup) begin
          jitter_detected <= 1'b1;
        end else begin
          jitter_detected <= 1'b0;
        end
      end else begin
        jitter_detected <= 1'b0;
      end
    end
  end

endmodule

