module huffman_encoder (
  input logic clk,
  input logic reset,
  input logic data_valid,
  input logic [4-1:0] data_in,
  input logic [2-1:0] data_priority,
  input logic update_enable,
  input logic [4-1:0] config_symbol,
  input logic [7-1:0] config_code,
  input logic [3-1:0] config_length,
  output logic [7-1:0] huffman_code_out,
  output logic code_valid,
  output logic error_flag
);

  // FSM states: 0=IDLE,1=PREPARE,2=CHECK_UPDATE,3=ENCODE,4=OUTPUT,5=HANDLE_ERROR,6=UPDATE_TABLE
  logic [3-1:0] state;
  // RAM interface wires
  logic ht_we;
  logic [4-1:0] ht_addr;
  logic [7-1:0] ht_din;
  logic [7-1:0] ht_dout;
  logic cl_we;
  logic [4-1:0] cl_addr;
  logic [3-1:0] cl_din;
  logic [3-1:0] cl_dout;
  logic qh_we;
  logic [4-1:0] qh_addr;
  logic [4-1:0] qh_din;
  logic [4-1:0] qh_dout;
  logic qm_we;
  logic [4-1:0] qm_addr;
  logic [4-1:0] qm_din;
  logic [4-1:0] qm_dout;
  logic ql_we;
  logic [4-1:0] ql_addr;
  logic [4-1:0] ql_din;
  logic [4-1:0] ql_dout;
  // Queue write pointers
  logic [4-1:0] qh_wptr;
  logic [4-1:0] qm_wptr;
  logic [4-1:0] ql_wptr;
  // Queue read pointers
  logic [4-1:0] qh_rptr;
  logic [4-1:0] qm_rptr;
  logic [4-1:0] ql_rptr;
  // Saved symbol for encoding
  logic [4-1:0] cur_symbol;
  // Saved update params
  logic [4-1:0] upd_symbol;
  logic [7-1:0] upd_code;
  logic [3-1:0] upd_length;
  logic upd_pending;
  // RAM instances
  single_port_ram #(.DATA_WIDTH(7), .ADDR_WIDTH(4)) ht_ram (
    .clk(clk),
    .we(ht_we),
    .addr(ht_addr),
    .din(ht_din),
    .dout(ht_dout)
  );
  single_port_ram #(.DATA_WIDTH(3), .ADDR_WIDTH(4)) cl_ram (
    .clk(clk),
    .we(cl_we),
    .addr(cl_addr),
    .din(cl_din),
    .dout(cl_dout)
  );
  single_port_ram #(.DATA_WIDTH(4), .ADDR_WIDTH(4)) qh_ram (
    .clk(clk),
    .we(qh_we),
    .addr(qh_addr),
    .din(qh_din),
    .dout(qh_dout)
  );
  single_port_ram #(.DATA_WIDTH(4), .ADDR_WIDTH(4)) qm_ram (
    .clk(clk),
    .we(qm_we),
    .addr(qm_addr),
    .din(qm_din),
    .dout(qm_dout)
  );
  single_port_ram #(.DATA_WIDTH(4), .ADDR_WIDTH(4)) ql_ram (
    .clk(clk),
    .we(ql_we),
    .addr(ql_addr),
    .din(ql_din),
    .dout(ql_dout)
  );
  // Default RAM drive signals (combinational)
  always_comb begin
    ht_we = 1'b0;
    ht_addr = 0;
    ht_din = 0;
    cl_we = 1'b0;
    cl_addr = 0;
    cl_din = 0;
    qh_we = 1'b0;
    qh_addr = 0;
    qh_din = 0;
    qm_we = 1'b0;
    qm_addr = 0;
    qm_din = 0;
    ql_we = 1'b0;
    ql_addr = 0;
    ql_din = 0;
    if (state == 0) begin
      // IDLE: enqueue data if valid
      if (data_valid) begin
        if (data_priority == 3) begin
          qh_we = 1'b1;
          qh_addr = qh_wptr;
          qh_din = data_in;
        end else if (data_priority == 2) begin
          qm_we = 1'b1;
          qm_addr = qm_wptr;
          qm_din = data_in;
        end else begin
          ql_we = 1'b1;
          ql_addr = ql_wptr;
          ql_din = data_in;
        end
      end
    end else if (state == 1) begin
      // PREPARE: read from highest priority queue
      if (qh_rptr != qh_wptr) begin
        qh_addr = qh_rptr;
      end else if (qm_rptr != qm_wptr) begin
        qm_addr = qm_rptr;
      end else if (ql_rptr != ql_wptr) begin
        ql_addr = ql_rptr;
      end
    end else if (state == 2) begin
      // CHECK_UPDATE
      ht_addr = upd_symbol;
      cl_addr = upd_symbol;
    end else if (state == 3) begin
      // ENCODE: read huffman table for current symbol
      ht_addr = cur_symbol;
      cl_addr = cur_symbol;
    end else if (state == 6) begin
      // UPDATE_TABLE: write new code and length
      ht_we = 1'b1;
      ht_addr = upd_symbol;
      ht_din = upd_code;
      cl_we = 1'b1;
      cl_addr = upd_symbol;
      cl_din = upd_length;
    end
  end
  // FSM sequential logic
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      code_valid <= 1'b0;
      cur_symbol <= 0;
      error_flag <= 1'b0;
      huffman_code_out <= 0;
      qh_rptr <= 0;
      qh_wptr <= 0;
      ql_rptr <= 0;
      ql_wptr <= 0;
      qm_rptr <= 0;
      qm_wptr <= 0;
      state <= 0;
      upd_code <= 0;
      upd_length <= 0;
      upd_pending <= 1'b0;
      upd_symbol <= 0;
    end else begin
      if (state == 0) begin
        // IDLE
        code_valid <= 1'b0;
        error_flag <= 1'b0;
        if (update_enable) begin
          upd_symbol <= config_symbol;
          upd_code <= config_code;
          upd_length <= config_length;
          upd_pending <= 1'b1;
          state <= 2;
        end else if (data_valid) begin
          if (data_priority == 3) begin
            qh_wptr <= 4'(qh_wptr + 1);
          end else if (data_priority == 2) begin
            qm_wptr <= 4'(qm_wptr + 1);
          end else begin
            ql_wptr <= 4'(ql_wptr + 1);
          end
          state <= 1;
        end
      end else if (state == 1) begin
        // PREPARE
        if (qh_rptr != qh_wptr) begin
          cur_symbol <= qh_dout;
          qh_rptr <= 4'(qh_rptr + 1);
          state <= 3;
        end else if (qm_rptr != qm_wptr) begin
          cur_symbol <= qm_dout;
          qm_rptr <= 4'(qm_rptr + 1);
          state <= 3;
        end else if (ql_rptr != ql_wptr) begin
          cur_symbol <= ql_dout;
          ql_rptr <= 4'(ql_rptr + 1);
          state <= 3;
        end else begin
          state <= 0;
        end
      end else if (state == 2) begin
        // CHECK_UPDATE
        if (upd_length == 0) begin
          error_flag <= 1'b1;
          state <= 5;
        end else begin
          state <= 6;
        end
      end else if (state == 3) begin
        // ENCODE
        state <= 4;
      end else if (state == 4) begin
        // OUTPUT
        huffman_code_out <= ht_dout;
        code_valid <= 1'b1;
        state <= 0;
      end else if (state == 5) begin
        // HANDLE_ERROR
        state <= 0;
      end else if (state == 6) begin
        // UPDATE_TABLE
        upd_pending <= 1'b0;
        state <= 0;
      end
    end
  end

endmodule

module single_port_ram #(
  parameter int DATA_WIDTH = 8,
  parameter int ADDR_WIDTH = 4
) (
  input  logic                    clk,
  input  logic                    we,
  input  logic [ADDR_WIDTH-1:0]   addr,
  input  logic [DATA_WIDTH-1:0]   din,
  output logic [DATA_WIDTH-1:0]   dout
);

  logic [DATA_WIDTH-1:0] mem [0:(1<<ADDR_WIDTH)-1];

  always_ff @(posedge clk) begin
    if (we)
      mem[addr] <= din;
    dout <= mem[addr];
  end

endmodule
