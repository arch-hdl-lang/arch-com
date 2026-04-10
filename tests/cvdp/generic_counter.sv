module generic_counter #(
  parameter int N = 8
) (
  input logic clk_in,
  input logic rst_in,
  input logic enable_in,
  input logic [3-1:0] mode_in,
  input logic [N-1:0] ref_modulo,
  output logic [N-1:0] o_count
);

  logic [N-1:0] cnt;
  always_ff @(posedge clk_in or posedge rst_in) begin
    if (rst_in) begin
      cnt <= 0;
    end else begin
      if (enable_in) begin
        if (mode_in == 3'd0) begin
          cnt <= N'(cnt + 1'd1);
        end else if (mode_in == 3'd1) begin
          cnt <= N'(cnt - 1'd1);
        end else if (mode_in == 3'd2) begin
          if (cnt >= ref_modulo) begin
            cnt <= 0;
          end else begin
            cnt <= N'(cnt + 1'd1);
          end
        end else if (mode_in == 3'd3) begin
          cnt <= {~cnt[0], cnt[N - 1:1]};
        end else if (mode_in == 3'd4) begin
          cnt <= N'(cnt + 1'd1);
        end else if (mode_in == 3'd5) begin
          if (cnt == 0) begin
            cnt <= 1;
          end else begin
            cnt <= {cnt[N - 2:0], cnt[N - 1]};
          end
        end
      end
    end
  end
  always_comb begin
    if (mode_in == 3'd4) begin
      o_count = cnt ^ cnt >> 1;
    end else begin
      o_count = cnt;
    end
  end

endmodule

