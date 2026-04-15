module hmac_reg_interface #(
  parameter int DATA_WIDTH = 8,
  parameter int ADDR_WIDTH = 8
) (
  input logic clk,
  input logic rst_n,
  input logic write_en,
  input logic read_en,
  input logic [ADDR_WIDTH-1:0] addr,
  input logic [DATA_WIDTH-1:0] wdata,
  input logic i_wait_en,
  output logic [DATA_WIDTH-1:0] rdata,
  output logic hmac_valid,
  output logic hmac_key_error
);

  // FSM state encoding
  logic [2:0] ST_IDLE;
  assign ST_IDLE = 0;
  logic [2:0] ST_ANALYZE;
  assign ST_ANALYZE = 1;
  logic [2:0] ST_XOR_DATA;
  assign ST_XOR_DATA = 2;
  logic [2:0] ST_WRITE;
  assign ST_WRITE = 3;
  logic [2:0] ST_LOST;
  assign ST_LOST = 4;
  logic [2:0] ST_CHECK_KEY;
  assign ST_CHECK_KEY = 5;
  logic [2:0] ST_TRIG_WAIT;
  assign ST_TRIG_WAIT = 6;
  logic [2:0] current_state;
  logic [DATA_WIDTH-1:0] hmac_key;
  logic [DATA_WIDTH-1:0] hmac_data;
  logic [255:0] [DATA_WIDTH-1:0] registers;
  // xor_data: current wdata XOR'd with '01010101...' mask — exposed for testbench visibility.
  // The Python model sets processed_data = wdata (plain) for all states except PROCESS/XOR_DATA,
  // and in WRITE state it immediately uses that overwritten value. This means the WRITE state
  // always writes the current wdata directly, not a previously XOR'd value.
  logic [DATA_WIDTH-1:0] xor_data;
  logic [DATA_WIDTH-1:0] xor_mask;
  always_comb begin
    xor_mask = DATA_WIDTH'($unsigned(0));
    for (int i = 0; i <= DATA_WIDTH / 2 - 1; i++) begin
      xor_mask[i * 2 +: 2] = 2'd1;
    end
  end
  assign xor_data = wdata ^ xor_mask;
  // Key validation: 2 MSB and 2 LSB of hmac_key must be zero for key to be valid
  logic key_valid;
  assign key_valid = (hmac_key[DATA_WIDTH - 2 +: 2] == 0) & (hmac_key[1:0] == 0);
  // Next-cycle hmac_key (what hmac_key will be after the next clock edge)
  // Used so hmac_key_error is updated in same cycle as hmac_key write
  logic [DATA_WIDTH-1:0] next_hmac_key;
  always_comb begin
    if ((current_state == ST_WRITE) & (addr == 0)) begin
      next_hmac_key = wdata;
    end else begin
      next_hmac_key = hmac_key;
    end
  end
  logic next_key_valid;
  assign next_key_valid = (next_hmac_key[DATA_WIDTH - 2 +: 2] == 0) & (next_hmac_key[1:0] == 0);
  // Next state combinational logic
  logic [2:0] next_state;
  always_comb begin
    if (current_state == ST_IDLE) begin
      if (write_en) begin
        next_state = ST_ANALYZE;
      end else begin
        next_state = ST_IDLE;
      end
    end else if (current_state == ST_ANALYZE) begin
      if (wdata[DATA_WIDTH - 1 +: 1] == 1) begin
        next_state = ST_XOR_DATA;
      end else begin
        next_state = ST_WRITE;
      end
    end else if (current_state == ST_XOR_DATA) begin
      next_state = ST_WRITE;
    end else if (current_state == ST_WRITE) begin
      if (write_en) begin
        next_state = ST_IDLE;
      end else begin
        next_state = ST_LOST;
      end
    end else if (current_state == ST_LOST) begin
      if (read_en) begin
        next_state = ST_CHECK_KEY;
      end else begin
        next_state = ST_LOST;
      end
    end else if (current_state == ST_CHECK_KEY) begin
      if (key_valid) begin
        next_state = ST_TRIG_WAIT;
      end else begin
        next_state = ST_WRITE;
      end
    end else if (current_state == ST_TRIG_WAIT) begin
      if (i_wait_en) begin
        next_state = ST_TRIG_WAIT;
      end else if ((hmac_data != 0) & (hmac_key != 0)) begin
        next_state = ST_IDLE;
      end else begin
        next_state = ST_WRITE;
      end
    end else begin
      next_state = ST_IDLE;
    end
  end
  // Sequential: state register
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      current_state <= 0;
    end else begin
      current_state <= next_state;
    end
  end
  // Sequential: key error flag — updated with look-ahead so it reflects
  // the key value being written in WRITE state in the same cycle
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      hmac_key_error <= 1'b0;
    end else begin
      hmac_key_error <= ~next_key_valid;
    end
  end
  // Sequential: Write and valid logic
  // The model writes wdata (current cycle's value) directly in WRITE state.
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      hmac_data <= 0;
      hmac_key <= 0;
      hmac_valid <= 1'b0;
      for (int __ri0 = 0; __ri0 < 256; __ri0++) begin
        registers[__ri0] <= 0;
      end
    end else begin
      hmac_valid <= 1'b0;
      if (current_state == ST_WRITE) begin
        if (addr == 0) begin
          hmac_key <= wdata;
        end else if (addr == 1) begin
          hmac_data <= wdata;
          hmac_valid <= 1'b1;
        end else begin
          registers[addr] <= wdata;
        end
      end
    end
  end
  // Sequential: Read logic
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      rdata <= 0;
    end else begin
      if (read_en & (current_state != ST_WRITE)) begin
        if (addr == 0) begin
          rdata <= hmac_key;
        end else if (addr == 1) begin
          rdata <= hmac_data;
        end else begin
          rdata <= registers[addr];
        end
      end
    end
  end

endmodule

