module bcd_counter (
  input logic clk,
  input logic rst,
  output logic [3:0] ms_hr,
  output logic [3:0] ls_hr,
  output logic [3:0] ms_min,
  output logic [3:0] ls_min,
  output logic [3:0] ms_sec,
  output logic [3:0] ls_sec
);

  always_ff @(posedge clk) begin
    if (rst) begin
      ls_hr <= 0;
      ls_min <= 0;
      ls_sec <= 0;
      ms_hr <= 0;
      ms_min <= 0;
      ms_sec <= 0;
    end else begin
      if (ls_sec < 4'd9) begin
        ls_sec <= 4'(ls_sec + 4'd1);
      end else begin
        ls_sec <= 4'd0;
        if (ms_sec < 4'd5) begin
          ms_sec <= 4'(ms_sec + 4'd1);
        end else begin
          ms_sec <= 4'd0;
          if (ls_min < 4'd9) begin
            ls_min <= 4'(ls_min + 4'd1);
          end else begin
            ls_min <= 4'd0;
            if (ms_min < 4'd5) begin
              ms_min <= 4'(ms_min + 4'd1);
            end else begin
              ms_min <= 4'd0;
              if (ms_hr == 4'd2 && ls_hr == 4'd3) begin
                ms_hr <= 4'd0;
                ls_hr <= 4'd0;
              end else if (ls_hr < 4'd9) begin
                ls_hr <= 4'(ls_hr + 4'd1);
              end else begin
                ls_hr <= 4'd0;
                ms_hr <= 4'(ms_hr + 4'd1);
              end
            end
          end
        end
      end
    end
  end

endmodule

