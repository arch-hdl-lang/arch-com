module single_port_ram__DATA_WIDTH_7 #(
  parameter int DATA_WIDTH = 7,
  parameter int ADDR_WIDTH = 4
) (
  input logic clk,
  input logic we,
  input logic [ADDR_WIDTH-1:0] addr,
  input logic [DATA_WIDTH-1:0] din,
  output logic [DATA_WIDTH-1:0] dout
);

  logic [15:0] [DATA_WIDTH-1:0] mem;
  always_ff @(posedge clk) begin
    if (we) begin
      mem[4'(addr)] <= din;
    end
    dout <= mem[4'(addr)];
  end
  // synopsys translate_off
  // Auto-generated safety assertions (bounds / divide-by-zero)
  _auto_bound_vec_0: assert property (@(posedge clk) (4'(addr)) < (16))
    else $fatal(1, "BOUNDS VIOLATION: single_port_ram__DATA_WIDTH_7._auto_bound_vec_0");
  // synopsys translate_on

endmodule

module single_port_ram__DATA_WIDTH_3 #(
  parameter int DATA_WIDTH = 3,
  parameter int ADDR_WIDTH = 4
) (
  input logic clk,
  input logic we,
  input logic [ADDR_WIDTH-1:0] addr,
  input logic [DATA_WIDTH-1:0] din,
  output logic [DATA_WIDTH-1:0] dout
);

  logic [15:0] [DATA_WIDTH-1:0] mem;
  always_ff @(posedge clk) begin
    if (we) begin
      mem[4'(addr)] <= din;
    end
    dout <= mem[4'(addr)];
  end
  // synopsys translate_off
  // Auto-generated safety assertions (bounds / divide-by-zero)
  _auto_bound_vec_0: assert property (@(posedge clk) (4'(addr)) < (16))
    else $fatal(1, "BOUNDS VIOLATION: single_port_ram__DATA_WIDTH_3._auto_bound_vec_0");
  // synopsys translate_on

endmodule

module single_port_ram__DATA_WIDTH_4 #(
  parameter int DATA_WIDTH = 4,
  parameter int ADDR_WIDTH = 4
) (
  input logic clk,
  input logic we,
  input logic [ADDR_WIDTH-1:0] addr,
  input logic [DATA_WIDTH-1:0] din,
  output logic [DATA_WIDTH-1:0] dout
);

  logic [15:0] [DATA_WIDTH-1:0] mem;
  always_ff @(posedge clk) begin
    if (we) begin
      mem[4'(addr)] <= din;
    end
    dout <= mem[4'(addr)];
  end
  // synopsys translate_off
  // Auto-generated safety assertions (bounds / divide-by-zero)
  _auto_bound_vec_0: assert property (@(posedge clk) (4'(addr)) < (16))
    else $fatal(1, "BOUNDS VIOLATION: single_port_ram__DATA_WIDTH_4._auto_bound_vec_0");
  // synopsys translate_on

endmodule

// 7-state Huffman encoder.
// State machine lives in `fsm huffman_encoder_core`; the wrapper
// `module huffman_encoder` owns the 5 RAM instances and connects
// them to the fsm. (`fsm` bodies don't host `inst` declarations.)
module huffman_encoder_core (
  input logic clk,
  input logic reset,
  input logic data_valid,
  input logic [3:0] data_in,
  input logic [1:0] data_priority,
  input logic update_enable,
  input logic [3:0] config_symbol,
  input logic [6:0] config_code,
  input logic [2:0] config_length,
  output logic [6:0] huffman_code_out,
  output logic code_valid,
  output logic error_flag,
  output logic ht_we,
  output logic [3:0] ht_addr,
  output logic [6:0] ht_din,
  input logic [6:0] ht_dout,
  output logic cl_we,
  output logic [3:0] cl_addr,
  output logic [2:0] cl_din,
  input logic [2:0] cl_dout,
  output logic qh_we,
  output logic [3:0] qh_addr,
  output logic [3:0] qh_din,
  input logic [3:0] qh_dout,
  output logic qm_we,
  output logic [3:0] qm_addr,
  output logic [3:0] qm_din,
  input logic [3:0] qm_dout,
  output logic ql_we,
  output logic [3:0] ql_addr,
  output logic [3:0] ql_din,
  input logic [3:0] ql_dout
);

  typedef enum logic [2:0] {
    IDLE = 3'd0,
    PREPARE = 3'd1,
    CHECK_UPDATE = 3'd2,
    ENCODE = 3'd3,
    OUTPUT = 3'd4,
    HANDLE_ERROR = 3'd5,
    UPDATE_TABLE = 3'd6
  } huffman_encoder_core_state_t;
  
  huffman_encoder_core_state_t state_r, state_next;
  
  logic [3:0] qh_wptr;
  logic [3:0] qm_wptr;
  logic [3:0] ql_wptr;
  logic [3:0] qh_rptr;
  logic [3:0] qm_rptr;
  logic [3:0] ql_rptr;
  logic [3:0] cur_symbol;
  logic [3:0] upd_symbol;
  logic [6:0] upd_code;
  logic [2:0] upd_length;
  logic upd_pending;
  
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      state_r <= IDLE;
      qh_wptr <= 0;
      qm_wptr <= 0;
      ql_wptr <= 0;
      qh_rptr <= 0;
      qm_rptr <= 0;
      ql_rptr <= 0;
      cur_symbol <= 0;
      upd_symbol <= 0;
      upd_code <= 0;
      upd_length <= 0;
      upd_pending <= 1'b0;
      huffman_code_out <= 0;
      code_valid <= 1'b0;
      error_flag <= 1'b0;
    end else begin
      state_r <= state_next;
      case (state_r)
        IDLE: begin
          // External I/O (mirrors top-level)
          // RAM driver ports (combinational outputs to the wrapper).
          // Datapath regs alongside the FSM state.
          // RAM idle defaults — every state overrides what it needs.
          // Enqueue incoming data into its priority queue.
          code_valid <= 1'b0;
          error_flag <= 1'b0;
          if (update_enable) begin
            upd_symbol <= config_symbol;
            upd_code <= config_code;
            upd_length <= config_length;
            upd_pending <= 1'b1;
          end else if (data_valid) begin
            if (data_priority == 3) begin
              qh_wptr <= 4'(qh_wptr + 1);
            end else if (data_priority == 2) begin
              qm_wptr <= 4'(qm_wptr + 1);
            end else begin
              ql_wptr <= 4'(ql_wptr + 1);
            end
          end
        end
        PREPARE: begin
          // Address the highest-priority non-empty queue.
          if (qh_rptr != qh_wptr) begin
            cur_symbol <= qh_dout;
            qh_rptr <= 4'(qh_rptr + 1);
          end else if (qm_rptr != qm_wptr) begin
            cur_symbol <= qm_dout;
            qm_rptr <= 4'(qm_rptr + 1);
          end else if (ql_rptr != ql_wptr) begin
            cur_symbol <= ql_dout;
            ql_rptr <= 4'(ql_rptr + 1);
          end
        end
        CHECK_UPDATE: begin
          if (upd_length == 0) begin
            error_flag <= 1'b1;
          end
        end
        OUTPUT: begin
          huffman_code_out <= ht_dout;
          code_valid <= 1'b1;
        end
        UPDATE_TABLE: begin
          upd_pending <= 1'b0;
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      IDLE: begin
        if (update_enable) state_next = CHECK_UPDATE;
        else if (!update_enable & data_valid) state_next = PREPARE;
      end
      PREPARE: begin
        if ((qh_rptr != qh_wptr) | (qm_rptr != qm_wptr) | (ql_rptr != ql_wptr)) state_next = ENCODE;
        else if ((qh_rptr == qh_wptr) & (qm_rptr == qm_wptr) & (ql_rptr == ql_wptr)) state_next = IDLE;
      end
      CHECK_UPDATE: begin
        if (upd_length == 0) state_next = HANDLE_ERROR;
        else if (upd_length != 0) state_next = UPDATE_TABLE;
      end
      ENCODE: begin
        state_next = OUTPUT;
      end
      OUTPUT: begin
        state_next = IDLE;
      end
      HANDLE_ERROR: begin
        state_next = IDLE;
      end
      UPDATE_TABLE: begin
        state_next = IDLE;
      end
      default: state_next = state_r;
    endcase
  end
  
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
    case (state_r)
      IDLE: begin
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
      end
      PREPARE: begin
        if (qh_rptr != qh_wptr) begin
          qh_addr = qh_rptr;
        end else if (qm_rptr != qm_wptr) begin
          qm_addr = qm_rptr;
        end else if (ql_rptr != ql_wptr) begin
          ql_addr = ql_rptr;
        end
      end
      CHECK_UPDATE: begin
        ht_addr = upd_symbol;
        cl_addr = upd_symbol;
      end
      ENCODE: begin
        ht_addr = cur_symbol;
        cl_addr = cur_symbol;
      end
      OUTPUT: begin
      end
      HANDLE_ERROR: begin
      end
      UPDATE_TABLE: begin
        ht_we = 1'b1;
        ht_addr = upd_symbol;
        ht_din = upd_code;
        cl_we = 1'b1;
        cl_addr = upd_symbol;
        cl_din = upd_length;
      end
      default: ;
    endcase
  end
  
  // synopsys translate_off
  _auto_legal_state: assert property (@(posedge clk) !reset |-> state_r < 7)
    else $fatal(1, "FSM ILLEGAL STATE: huffman_encoder_core.state_r = %0d", state_r);
  _auto_reach_IDLE: cover property (@(posedge clk) state_r == IDLE);
  _auto_reach_PREPARE: cover property (@(posedge clk) state_r == PREPARE);
  _auto_reach_CHECK_UPDATE: cover property (@(posedge clk) state_r == CHECK_UPDATE);
  _auto_reach_ENCODE: cover property (@(posedge clk) state_r == ENCODE);
  _auto_reach_OUTPUT: cover property (@(posedge clk) state_r == OUTPUT);
  _auto_reach_HANDLE_ERROR: cover property (@(posedge clk) state_r == HANDLE_ERROR);
  _auto_reach_UPDATE_TABLE: cover property (@(posedge clk) state_r == UPDATE_TABLE);
  _auto_tr_IDLE_to_CHECK_UPDATE: cover property (@(posedge clk) state_r == IDLE && state_next == CHECK_UPDATE);
  _auto_tr_IDLE_to_PREPARE: cover property (@(posedge clk) state_r == IDLE && state_next == PREPARE);
  _auto_tr_PREPARE_to_ENCODE: cover property (@(posedge clk) state_r == PREPARE && state_next == ENCODE);
  _auto_tr_PREPARE_to_IDLE: cover property (@(posedge clk) state_r == PREPARE && state_next == IDLE);
  _auto_tr_CHECK_UPDATE_to_HANDLE_ERROR: cover property (@(posedge clk) state_r == CHECK_UPDATE && state_next == HANDLE_ERROR);
  _auto_tr_CHECK_UPDATE_to_UPDATE_TABLE: cover property (@(posedge clk) state_r == CHECK_UPDATE && state_next == UPDATE_TABLE);
  _auto_tr_ENCODE_to_OUTPUT: cover property (@(posedge clk) state_r == ENCODE && state_next == OUTPUT);
  _auto_tr_OUTPUT_to_IDLE: cover property (@(posedge clk) state_r == OUTPUT && state_next == IDLE);
  _auto_tr_HANDLE_ERROR_to_IDLE: cover property (@(posedge clk) state_r == HANDLE_ERROR && state_next == IDLE);
  _auto_tr_UPDATE_TABLE_to_IDLE: cover property (@(posedge clk) state_r == UPDATE_TABLE && state_next == IDLE);
  // synopsys translate_on

endmodule

// Top-level wrapper: 5 single-port RAMs + 1 fsm core.
module huffman_encoder (
  input logic clk,
  input logic reset,
  input logic data_valid,
  input logic [3:0] data_in,
  input logic [1:0] data_priority,
  input logic update_enable,
  input logic [3:0] config_symbol,
  input logic [6:0] config_code,
  input logic [2:0] config_length,
  output logic [6:0] huffman_code_out,
  output logic code_valid,
  output logic error_flag
);

  // FSM ↔ RAM interconnect wires.
  logic ht_we;
  logic [3:0] ht_addr;
  logic [6:0] ht_din;
  logic [6:0] ht_dout;
  logic cl_we;
  logic [3:0] cl_addr;
  logic [2:0] cl_din;
  logic [2:0] cl_dout;
  logic qh_we;
  logic [3:0] qh_addr;
  logic [3:0] qh_din;
  logic [3:0] qh_dout;
  logic qm_we;
  logic [3:0] qm_addr;
  logic [3:0] qm_din;
  logic [3:0] qm_dout;
  logic ql_we;
  logic [3:0] ql_addr;
  logic [3:0] ql_din;
  logic [3:0] ql_dout;
  single_port_ram__DATA_WIDTH_7 #(.DATA_WIDTH(7), .ADDR_WIDTH(4)) ht_ram (
    .clk(clk),
    .we(ht_we),
    .addr(ht_addr),
    .din(ht_din),
    .dout(ht_dout)
  );
  single_port_ram__DATA_WIDTH_3 #(.DATA_WIDTH(3), .ADDR_WIDTH(4)) cl_ram (
    .clk(clk),
    .we(cl_we),
    .addr(cl_addr),
    .din(cl_din),
    .dout(cl_dout)
  );
  single_port_ram__DATA_WIDTH_4 #(.DATA_WIDTH(4), .ADDR_WIDTH(4)) qh_ram (
    .clk(clk),
    .we(qh_we),
    .addr(qh_addr),
    .din(qh_din),
    .dout(qh_dout)
  );
  single_port_ram__DATA_WIDTH_4 #(.DATA_WIDTH(4), .ADDR_WIDTH(4)) qm_ram (
    .clk(clk),
    .we(qm_we),
    .addr(qm_addr),
    .din(qm_din),
    .dout(qm_dout)
  );
  single_port_ram__DATA_WIDTH_4 #(.DATA_WIDTH(4), .ADDR_WIDTH(4)) ql_ram (
    .clk(clk),
    .we(ql_we),
    .addr(ql_addr),
    .din(ql_din),
    .dout(ql_dout)
  );
  huffman_encoder_core core (
    .clk(clk),
    .reset(reset),
    .data_valid(data_valid),
    .data_in(data_in),
    .data_priority(data_priority),
    .update_enable(update_enable),
    .config_symbol(config_symbol),
    .config_code(config_code),
    .config_length(config_length),
    .huffman_code_out(huffman_code_out),
    .code_valid(code_valid),
    .error_flag(error_flag),
    .ht_we(ht_we),
    .ht_addr(ht_addr),
    .ht_din(ht_din),
    .ht_dout(ht_dout),
    .cl_we(cl_we),
    .cl_addr(cl_addr),
    .cl_din(cl_din),
    .cl_dout(cl_dout),
    .qh_we(qh_we),
    .qh_addr(qh_addr),
    .qh_din(qh_din),
    .qh_dout(qh_dout),
    .qm_we(qm_we),
    .qm_addr(qm_addr),
    .qm_din(qm_din),
    .qm_dout(qm_dout),
    .ql_we(ql_we),
    .ql_addr(ql_addr),
    .ql_din(ql_din),
    .ql_dout(ql_dout)
  );

endmodule

