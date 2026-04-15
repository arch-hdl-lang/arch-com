module radix2_div (
  input logic clk,
  input logic rst_n,
  input logic start,
  input logic [7:0] dividend,
  input logic [7:0] divisor,
  output logic [7:0] quotient,
  output logic [7:0] remainder,
  output logic done
);

  logic [7:0] dvd;
  logic [7:0] dvs;
  logic [7:0] quot;
  logic [7:0] acc;
  logic [3:0] count;
  logic active;
  logic done_r;
  // trial: shift acc left 1, bring in MSB of dvd
  logic [8:0] trial;
  assign trial = {acc, dvd[7:7]};
  logic [8:0] sub_result;
  assign sub_result = 9'(trial - 9'($unsigned(dvs)));
  logic fits;
  assign fits = ~sub_result[8:8];
  assign quotient = quot;
  assign remainder = acc;
  assign done = done_r;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      acc <= 0;
      active <= 0;
      count <= 0;
      done_r <= 0;
      dvd <= 0;
      dvs <= 0;
      quot <= 0;
    end else begin
      done_r <= 1'b0;
      if (start) begin
        if (divisor == 0) begin
          quot <= 8'd255;
          acc <= 8'd255;
          active <= 1'b0;
          done_r <= 1'b1;
          dvd <= 8'($unsigned(0));
          dvs <= 8'($unsigned(0));
          count <= 4'($unsigned(0));
        end else begin
          dvd <= {dividend[6:0], 1'd0};
          dvs <= divisor;
          count <= 4'($unsigned(1));
          active <= 1'b1;
          // first iteration: trial = {0, dividend[7]}
          if (9'($unsigned(dividend[7:7])) >= 9'($unsigned(divisor))) begin
            acc <= 8'(9'($unsigned(dividend[7:7])) - 9'($unsigned(divisor)));
            quot <= 8'd1;
          end else begin
            acc <= 8'($unsigned(dividend[7:7]));
            quot <= 8'($unsigned(0));
          end
        end
      end else if (active) begin
        if (fits) begin
          acc <= 8'(sub_result);
          quot <= {quot[6:0], 1'd1};
        end else begin
          acc <= 8'(trial);
          quot <= {quot[6:0], 1'd0};
        end
        dvd <= {dvd[6:0], 1'd0};
        if (count == 7) begin
          active <= 1'b0;
          done_r <= 1'b1;
        end else begin
          count <= 4'(count + 1);
        end
      end
    end
  end

endmodule

