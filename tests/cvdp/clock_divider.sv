module clock_divider (
  input logic clk,
  input logic rst_n,
  input logic [2-1:0] sel,
  output logic clk_out
);

  logic [3-1:0] cnt;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      clk_out <= 0;
      cnt <= 0;
    end else begin
      if (sel == 2'd0) begin
        clk_out <= ~clk_out;
        cnt <= 3'd0;
      end else if (sel == 2'd1) begin
        if (cnt == 3'd1) begin
          clk_out <= ~clk_out;
          cnt <= 3'd0;
        end else begin
          cnt <= 3'(cnt + 3'd1);
        end
      end else if (sel == 2'd2) begin
        if (cnt == 3'd3) begin
          clk_out <= ~clk_out;
          cnt <= 3'd0;
        end else begin
          cnt <= 3'(cnt + 3'd1);
        end
      end else begin
        clk_out <= 1'd0;
        cnt <= 3'd0;
      end
    end
  end

endmodule

// Divide by 2: toggle every cycle
// Divide by 4: toggle every 2 cycles
// Divide by 8: toggle every 4 cycles
