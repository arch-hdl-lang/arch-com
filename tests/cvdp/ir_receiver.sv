module ir_receiver (
  input logic clk_in,
  input logic reset_in,
  input logic ir_signal_in,
  output logic [11:0] ir_frame_out,
  output logic ir_frame_valid,
  output logic [4:0] ir_device_address_out,
  output logic [6:0] ir_function_code_out,
  output logic ir_output_valid
);

  logic [2:0] present_state;
  logic [15:0] cycle_cnt;
  logic [3:0] bit_cnt;
  logic [11:0] ir_frame_reg;
  logic [4:0] raw_address;
  assign raw_address = ir_frame_out[11:7];
  logic [6:0] raw_function;
  assign raw_function = ir_frame_out[6:0];
  logic [4:0] decoded_address;
  logic [6:0] decoded_function;
  always_comb begin
    if (raw_function < 10) begin
      decoded_address = 5'($unsigned(1)) << raw_address;
      if (raw_function == 9) begin
        decoded_function = 0;
      end else begin
        decoded_function = 7'(raw_function + 1);
      end
    end else if ((raw_function >= 16) & (raw_function < 23)) begin
      decoded_address = 5'($unsigned(1)) << raw_address;
      decoded_function = 3'(raw_function - 15) << 4 | 7'd15;
    end else begin
      decoded_address = 0;
      decoded_function = 0;
    end
  end
  always_comb begin
    if (reset_in) begin
      ir_device_address_out = 0;
      ir_function_code_out = 0;
      ir_output_valid = 1'b0;
    end else begin
      ir_device_address_out = decoded_address;
      ir_function_code_out = decoded_function;
      ir_output_valid = ir_frame_valid;
    end
  end
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
        ir_frame_valid <= 1'b0;
        if (ir_signal_in) begin
          cycle_cnt <= 16'(cycle_cnt + 1);
        end else if ((cycle_cnt >= 18) & (cycle_cnt < 31)) begin
          cycle_cnt <= 0;
          present_state <= 2;
        end else begin
          present_state <= 0;
          cycle_cnt <= 0;
        end
      end else if (present_state == 2) begin
        ir_frame_valid <= 1'b0;
        if (ir_signal_in) begin
          if ((cycle_cnt >= 3) & (cycle_cnt < 10)) begin
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
        ir_frame_valid <= 1'b0;
        if (ir_signal_in) begin
          cycle_cnt <= 16'(cycle_cnt + 1);
        end else if ((cycle_cnt >= 9) & (cycle_cnt < 16)) begin
          ir_frame_reg <= ir_frame_reg | 12'($unsigned(1)) << bit_cnt;
          bit_cnt <= 4'(bit_cnt + 1);
          cycle_cnt <= 0;
          if (bit_cnt == 11) begin
            present_state <= 4;
          end else begin
            present_state <= 2;
          end
        end else if ((cycle_cnt >= 3) & (cycle_cnt < 10)) begin
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

