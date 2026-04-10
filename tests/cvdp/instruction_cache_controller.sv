module instruction_cache_controller #(
  parameter int TAG_BITS = 8,
  parameter int ADR_BITS = 9
) (
  input logic clk,
  input logic rst,
  output logic io_mem_valid,
  input logic io_mem_ready,
  output logic [17-1:0] io_mem_addr,
  output logic l1b_wait,
  output logic [32-1:0] l1b_data,
  input logic [18-1:0] l1b_addr,
  output logic ram256_t0_we,
  output logic [8-1:0] ram256_t0_addr,
  input logic [8-1:0] ram256_t0_data,
  output logic ram256_t1_we,
  output logic [8-1:0] ram256_t1_addr,
  input logic [8-1:0] ram256_t1_data,
  output logic ram512_d0_we,
  output logic [9-1:0] ram512_d0_addr,
  input logic [16-1:0] ram512_d0_data,
  output logic ram512_d1_we,
  output logic [9-1:0] ram512_d1_addr,
  input logic [16-1:0] ram512_d1_data
);

  // Unaligned data handling
  logic [16-1:0] data_0;
  logic [16-1:0] data_1;
  always_comb begin
    if (l1b_addr[0]) begin
      data_0 = ram512_d1_data;
      data_1 = ram512_d0_data;
    end else begin
      data_0 = ram512_d0_data;
      data_1 = ram512_d1_data;
    end
    l1b_data = {data_1, data_0};
  end
  // FSM states: IDLE=0, READMEM0=1, READMEM1=2, READCACHE=3
  logic [3-1:0] state_r;
  logic [9-1:0] addr_0_r;
  logic [9-1:0] addr_1_r;
  logic write_en_r;
  logic [9-1:0] data_addr_0;
  assign data_addr_0 = 9'(l1b_addr[17:9] + {8'd0, l1b_addr[0]});
  logic [9-1:0] data_addr_1;
  assign data_addr_1 = l1b_addr[17:9];
  // Tag controller outputs
  logic valid_0;
  logic [8-1:0] tag_0;
  logic valid_1;
  logic [8-1:0] tag_1;
  logic [9-1:0] tc_data_out_0;
  logic [9-1:0] tc_data_out_1;
  assign valid_0 = tc_data_out_0[8];
  assign tag_0 = tc_data_out_0[7:0];
  assign valid_1 = tc_data_out_1[8];
  assign tag_1 = tc_data_out_1[7:0];
  logic data_0_ready;
  assign data_0_ready = l1b_addr[17:9] == 9'($unsigned(tag_0)) & valid_0;
  logic data_1_ready;
  assign data_1_ready = l1b_addr[17:9] == 9'($unsigned(tag_1)) & valid_1;
  // Next state + output logic (all wires)
  logic [3-1:0] next_state;
  logic mem_valid_w;
  logic [17-1:0] mem_addr_w;
  logic wait_w;
  logic d0_we_w;
  logic d1_we_w;
  always_comb begin
    next_state = 0;
    mem_valid_w = 1'b0;
    mem_addr_w = 0;
    wait_w = 1'b0;
    d0_we_w = 1'b0;
    d1_we_w = 1'b0;
    if (state_r == 0) begin
      // IDLE
      if (data_0_ready & data_1_ready) begin
        wait_w = 1'b0;
        next_state = 0;
      end else begin
        wait_w = 1'b1;
        next_state = 1;
        mem_valid_w = 1'b1;
        mem_addr_w = {1'd0, l1b_addr[17:9], 7'd0};
      end
    end else if (state_r == 1) begin
      // READMEM0
      wait_w = 1'b1;
      mem_valid_w = 1'b1;
      mem_addr_w = {1'd0, addr_1_r, 7'd0};
      if (io_mem_ready) begin
        d0_we_w = 1'b1;
        next_state = 2;
      end else begin
        next_state = 1;
      end
    end else if (state_r == 2) begin
      // READMEM1
      wait_w = 1'b1;
      mem_valid_w = 1'b1;
      mem_addr_w = {1'd0, addr_0_r, 7'd0};
      if (io_mem_ready) begin
        d1_we_w = 1'b1;
        next_state = 3;
      end else begin
        next_state = 2;
      end
    end else if (state_r == 3) begin
      // READCACHE
      wait_w = 1'b0;
      next_state = 0;
    end else begin
      next_state = 0;
    end
    io_mem_valid = mem_valid_w;
    io_mem_addr = mem_addr_w;
    l1b_wait = wait_w;
    ram512_d0_we = d0_we_w;
    ram512_d1_we = d1_we_w;
    ram512_d0_addr = addr_0_r;
    ram512_d1_addr = addr_1_r;
  end
  // Sequential logic
  always_ff @(posedge clk or negedge rst) begin
    if ((!rst)) begin
      addr_0_r <= 0;
      addr_1_r <= 0;
      state_r <= 0;
      write_en_r <= 1'b0;
    end else begin
      if (state_r == 1 | state_r == 2) begin
        if (io_mem_ready) begin
          write_en_r <= 1'b1;
        end else begin
          write_en_r <= 1'b0;
        end
      end else begin
        write_en_r <= 1'b0;
      end
      state_r <= next_state;
      addr_0_r <= data_addr_0;
      addr_1_r <= data_addr_1;
    end
  end
  // Tag controller instance
  tag_controller tag_ctrl (
    .clk(clk),
    .rst(rst),
    .write_enable(write_en_r),
    .write_addr(io_mem_addr[8:0]),
    .read_addr_0(data_addr_0[7:0]),
    .read_addr_1(data_addr_1[7:0]),
    .data_in_0(ram256_t0_data),
    .data_in_1(ram256_t1_data),
    .data_out_0(tc_data_out_0),
    .data_out_1(tc_data_out_1),
    .write_enable_0(ram256_t0_we),
    .write_enable_1(ram256_t1_we),
    .addr_out_0(ram256_t0_addr),
    .addr_out_1(ram256_t1_addr)
  );

endmodule

