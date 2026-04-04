module radix2_div (
  input logic clk,
  input logic rst_n,
  input logic start,
  input logic [8-1:0] dividend,
  input logic [8-1:0] divisor,
  output logic [8-1:0] quotient,
  output logic [8-1:0] remainder,
  output logic done
);

  logic [8-1:0] dvd;
  logic [8-1:0] dvs;
  logic [8-1:0] quot;
  logic [9-1:0] partial;
  logic [4-1:0] bit_idx;
  logic active;
  logic done_reg;
  // Combinational helpers for the shift/subtract step
  // Shift partial left 1, bring in next dividend MSB
  logic [9-1:0] shifted;
  logic [8-1:0] p_upper;
  logic p_ge_dvs;
  assign shifted = {partial[7:0], dvd[7:7]};
  assign p_upper = shifted[8:1];
  assign p_ge_dvs = 9'($unsigned(p_upper)) >= 9'($unsigned(dvs));
  assign quotient = quot;
  assign remainder = partial[7:0];
  assign done = done_reg;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      active <= 1'b0;
      bit_idx <= 0;
      done_reg <= 1'b0;
      dvd <= 0;
      dvs <= 0;
      partial <= 0;
      quot <= 0;
    end else begin
      done_reg <= 1'b0;
      if (start) begin
        dvd <= dividend;
        dvs <= divisor;
        quot <= 8'($unsigned(0));
        partial <= 9'($unsigned(dividend[7:7]));
        bit_idx <= 4'($unsigned(0));
        active <= 1'b1;
      end else if (active) begin
        if (p_ge_dvs) begin
          partial <= 9'(9'($unsigned(p_upper)) - 9'($unsigned(dvs)));
          quot <= {quot[6:0], 1'd1};
        end else begin
          partial <= 9'($unsigned(p_upper));
          quot <= {quot[6:0], 1'd0};
        end
        dvd <= {dvd[6:0], 1'd0};
        if (bit_idx == 7) begin
          active <= 1'b0;
          done_reg <= 1'b1;
        end else begin
          bit_idx <= 4'(bit_idx + 1);
        end
      end
    end
  end

endmodule

