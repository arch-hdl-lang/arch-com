module secure_read_write_register_bank #(
  parameter int P_ADDRESS_WIDTH = 8,
  parameter int P_DATA_WIDTH = 8,
  parameter int P_UNLOCK_CODE_0 = 171,
  parameter int P_UNLOCK_CODE_1 = 205
) (
  input logic i_capture_pulse,
  input logic i_rst_n,
  input logic [P_ADDRESS_WIDTH-1:0] i_addr,
  input logic [P_DATA_WIDTH-1:0] i_data_in,
  input logic i_read_write_enable,
  output logic [P_DATA_WIDTH-1:0] o_data_out
);

  logic [2-1:0] lock_state;
  logic [P_DATA_WIDTH-1:0] mem [0:256-1];
  logic [2-1:0] LOCKED;
  assign LOCKED = 0;
  logic [2-1:0] GOT_CODE0;
  assign GOT_CODE0 = 1;
  logic [2-1:0] UNLOCKED;
  assign UNLOCKED = 2;
  logic is_write;
  assign is_write = i_read_write_enable == 0;
  logic is_read;
  assign is_read = i_read_write_enable == 1;
  always_ff @(posedge i_capture_pulse or negedge i_rst_n) begin
    if ((!i_rst_n)) begin
      lock_state <= 0;
    end else begin
      if (is_write) begin
        if (i_addr == 0) begin
          if (i_data_in == P_DATA_WIDTH'(P_UNLOCK_CODE_0)) begin
            lock_state <= GOT_CODE0;
          end else begin
            lock_state <= LOCKED;
          end
        end else if (i_addr == 1) begin
          if (lock_state == GOT_CODE0) begin
            if (i_data_in == P_DATA_WIDTH'(P_UNLOCK_CODE_1)) begin
              lock_state <= UNLOCKED;
            end else begin
              lock_state <= LOCKED;
            end
          end else if (i_data_in != P_DATA_WIDTH'(P_UNLOCK_CODE_1)) begin
            lock_state <= LOCKED;
          end else begin
            lock_state <= LOCKED;
          end
        end else if (lock_state == GOT_CODE0) begin
          lock_state <= LOCKED;
        end
      end else if (lock_state == GOT_CODE0) begin
        lock_state <= LOCKED;
      end
    end
  end
  always_ff @(posedge i_capture_pulse or negedge i_rst_n) begin
    if ((!i_rst_n)) begin
      for (int __ri0 = 0; __ri0 < 256; __ri0++) begin
        mem[__ri0] <= 0;
      end
      o_data_out <= 0;
    end else begin
      if (is_write) begin
        if (i_addr == 0) begin
          mem[0] <= i_data_in;
        end else if (i_addr == 1) begin
          mem[1] <= i_data_in;
        end else if (lock_state == UNLOCKED) begin
          mem[i_addr] <= i_data_in;
        end
        o_data_out <= 0;
      end else if (lock_state == UNLOCKED) begin
        if (i_addr == 0) begin
          o_data_out <= 0;
        end else if (i_addr == 1) begin
          o_data_out <= 0;
        end else begin
          o_data_out <= mem[i_addr];
        end
      end else begin
        o_data_out <= 0;
      end
    end
  end

endmodule

