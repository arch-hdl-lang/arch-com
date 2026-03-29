module arithmetic_progression_generator #(
  parameter int DATA_WIDTH = 16,
  parameter int SEQUENCE_LENGTH = 10,
  parameter int WIDTH_OUT_VAL = $clog2(SEQUENCE_LENGTH) + DATA_WIDTH
) (
  input logic clk,
  input logic resetn,
  input logic enable,
  input logic [DATA_WIDTH-1:0] start_val,
  input logic [DATA_WIDTH-1:0] step_size,
  output logic [WIDTH_OUT_VAL-1:0] out_val,
  output logic done
);

  logic [WIDTH_OUT_VAL-1:0] current_val;
  logic [$clog2(SEQUENCE_LENGTH)-1:0] cnt;
  logic done_r;
  logic [$clog2(SEQUENCE_LENGTH)-1:0] seq_limit;
  assign seq_limit = $clog2(SEQUENCE_LENGTH)'(SEQUENCE_LENGTH - 1);
  logic [WIDTH_OUT_VAL-1:0] step_ext;
  assign step_ext = WIDTH_OUT_VAL'($unsigned(step_size));
  always_ff @(posedge clk or negedge resetn) begin
    if ((!resetn)) begin
      cnt <= 0;
      current_val <= 0;
      done_r <= 1'b0;
    end else begin
      if (enable) begin
        if (done_r == 1'b0) begin
          if (cnt == 0) begin
            current_val <= WIDTH_OUT_VAL'($unsigned(start_val));
            cnt <= 1;
            if (SEQUENCE_LENGTH == 1) begin
              done_r <= 1'b1;
            end
          end else if (cnt == seq_limit) begin
            current_val <= WIDTH_OUT_VAL'(current_val + step_ext);
            cnt <= cnt;
            done_r <= 1'b1;
          end else begin
            current_val <= WIDTH_OUT_VAL'(current_val + step_ext);
            cnt <= $clog2(SEQUENCE_LENGTH)'(cnt + 1);
          end
        end
      end
    end
  end
  assign out_val = current_val;
  assign done = done_r;

endmodule

