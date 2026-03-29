module secure_variable_timer (
  input logic i_clk,
  input logic i_rst_n,
  input logic i_data_in,
  output logic [4-1:0] o_time_left,
  output logic o_processing,
  output logic o_completed,
  input logic i_ack
);

  // State encoding: 0=IDLE, 1=CONFIG, 2=COUNT, 3=DONE
  logic [2-1:0] st;
  logic [4-1:0] shift_reg;
  logic [3-1:0] bit_cnt;
  logic [4-1:0] delay_val;
  logic [10-1:0] cycle_cnt;
  // Pattern detection: track last 4 bits for 1101
  logic [4-1:0] pat;
  always_ff @(posedge i_clk) begin
    if ((!i_rst_n)) begin
      bit_cnt <= 0;
      cycle_cnt <= 0;
      delay_val <= 0;
      o_completed <= 1'b0;
      o_processing <= 1'b0;
      o_time_left <= 0;
      pat <= 0;
      shift_reg <= 0;
      st <= 0;
    end else begin
      if (i_rst_n) begin
        if (st == 0) begin
          // IDLE: detect 1101 pattern
          pat <= {pat[2:0], i_data_in};
          if (pat == 4'd13) begin
            st <= 1;
            shift_reg <= {3'd0, i_data_in};
            bit_cnt <= 1;
            pat <= 0;
          end
        end else if (st == 1) begin
          // CONFIG: shift in 4 bits MSB first
          shift_reg <= {shift_reg[2:0], i_data_in};
          bit_cnt <= 3'(bit_cnt + 1);
          if (bit_cnt == 3) begin
            st <= 2;
            delay_val <= {shift_reg[2:0], i_data_in};
            o_time_left <= {shift_reg[2:0], i_data_in};
            o_processing <= 1'b1;
            cycle_cnt <= 0;
          end
        end else if (st == 2) begin
          // COUNT
          if (cycle_cnt == 999) begin
            cycle_cnt <= 0;
            if (o_time_left == 0) begin
              st <= 3;
              o_processing <= 1'b0;
              o_completed <= 1'b1;
            end else begin
              o_time_left <= 4'(o_time_left - 1);
            end
          end else begin
            cycle_cnt <= 10'(cycle_cnt + 1);
          end
        end else if (i_ack) begin
          // DONE: wait for ack
          st <= 0;
          o_completed <= 1'b0;
          o_processing <= 1'b0;
          o_time_left <= 0;
          pat <= 0;
        end
      end else begin
        st <= 0;
        pat <= 0;
        shift_reg <= 0;
        bit_cnt <= 0;
        delay_val <= 0;
        cycle_cnt <= 0;
        o_time_left <= 0;
        o_processing <= 1'b0;
        o_completed <= 1'b0;
      end
    end
  end

endmodule

