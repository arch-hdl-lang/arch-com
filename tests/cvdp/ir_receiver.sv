module ir_receiver (
  input logic clk_in,
  input logic reset_in,
  input logic ir_signal_in,
  output logic [11:0] ir_frame_out,
  output logic ir_frame_valid
);

  logic [2:0] present_state;
  logic [15:0] cycle_cnt;
  logic [3:0] bit_cnt;
  logic [11:0] ir_frame_reg;
  always_ff @(posedge clk_in or posedge reset_in) begin
    if (reset_in) begin
      bit_cnt <= 0;
      cycle_cnt <= 0;
      ir_frame_out <= 0;
      ir_frame_reg <= 0;
      ir_frame_valid <= 1'b0;
      present_state <= 0;
    end else begin
      if (present_state == 0) begin
        ir_frame_valid <= 1'b0;
        cycle_cnt <= 0;
        bit_cnt <= 0;
        ir_frame_reg <= 0;
        if (ir_signal_in) begin
          present_state <= 1;
        end
      end else if (present_state == 1) begin
        // START: count HIGH pulse for start bit (~2400 cycles at 1MHz = 2.4ms)
        ir_frame_valid <= 1'b0;
        if (ir_signal_in) begin
          cycle_cnt <= 16'(cycle_cnt + 1);
        end else if ((cycle_cnt >= 1800) & (cycle_cnt < 3001)) begin
          cycle_cnt <= 0;
          present_state <= 2;
        end else begin
          present_state <= 0;
          cycle_cnt <= 0;
        end
      end else if (present_state == 2) begin
        // DECODE_LOW: count LOW pulse (~600 cycles at 1MHz = 0.6ms)
        ir_frame_valid <= 1'b0;
        if (ir_signal_in) begin
          if ((cycle_cnt >= 300) & (cycle_cnt < 901)) begin
            cycle_cnt <= 0;
            present_state <= 3;
          end else begin
            present_state <= 0;
            cycle_cnt <= 0;
          end
        end else begin
          cycle_cnt <= 16'(cycle_cnt + 1);
        end
      end else if (present_state == 3) begin
        // DECODE_HIGH: 0=~600cyc, 1=~1200cyc
        ir_frame_valid <= 1'b0;
        if (ir_signal_in) begin
          cycle_cnt <= 16'(cycle_cnt + 1);
        end else if ((cycle_cnt >= 901) & (cycle_cnt < 1501)) begin
          // bit is 1
          ir_frame_reg <= ir_frame_reg | 12'($unsigned(1)) << bit_cnt;
          bit_cnt <= 4'(bit_cnt + 1);
          cycle_cnt <= 0;
          if (bit_cnt == 11) begin
            present_state <= 4;
          end else begin
            present_state <= 2;
          end
        end else if ((cycle_cnt >= 300) & (cycle_cnt < 901)) begin
          // bit is 0
          bit_cnt <= 4'(bit_cnt + 1);
          cycle_cnt <= 0;
          if (bit_cnt == 11) begin
            present_state <= 4;
          end else begin
            present_state <= 2;
          end
        end else begin
          present_state <= 0;
          cycle_cnt <= 0;
        end
      end else if (present_state == 4) begin
        // DONE: output decoded frame for one cycle
        ir_frame_out <= ir_frame_reg;
        ir_frame_valid <= 1'b1;
        present_state <= 0;
      end else begin
        ir_frame_valid <= 1'b0;
        present_state <= 0;
      end
    end
  end

endmodule

