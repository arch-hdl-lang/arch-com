module Bitstream (
  input logic clk,
  input logic rst_n,
  input logic enb,
  input logic rempty_in,
  input logic rinc_in,
  input logic [7:0] i_byte,
  output logic o_bit,
  output logic rempty_out,
  output logic rinc_out,
  output logic [1:0] curr_state
);

  typedef enum logic [1:0] {
    IDLE = 2'd0,
    WAITR = 2'd1,
    READY = 2'd2
  } Bitstream_state_t;
  
  Bitstream_state_t state_r, state_next;
  
  logic [3:0] bp;
  logic [7:0] byte_buf;
  
  assign curr_state = state_r;
  logic rde;
  assign rde = bp[3];
  
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      state_r <= IDLE;
      bp <= 0;
      byte_buf <= 0;
    end else begin
      state_r <= state_next;
      // Mirror the encoded fsm state onto the user-declared output port.
      // Encoding follows declaration order (Idle=0, WaitR=1, Ready=2),
      // matching the original UInt<2> encoding.
      // rde: read-done flag (bp[3]=1 means all 8 bits consumed)
      // Datapath updates gated on the state's effective rinc_out value.
      if (rinc_out) begin
        byte_buf <= i_byte;
        bp <= 0;
      end else if (rinc_in & ~rempty_out) begin
        bp <= 4'(bp + 4'd1);
      end
      case (state_r)
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      IDLE: begin
        if (enb) state_next = WAITR;
      end
      WAITR: begin
        if (~rempty_in) state_next = READY;
      end
      READY: begin
        if (rde & rempty_in) state_next = WAITR;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    rinc_out = 1'b0;
    rempty_out = 1'b1;
    o_bit = byte_buf[bp[2:0]];
    case (state_r)
      IDLE: begin
      end
      WAITR: begin
        rinc_out = ~rempty_in;
      end
      READY: begin
        if (rde) begin
          rinc_out = ~rempty_in;
          rempty_out = 1'b1;
        end else begin
          rinc_out = 1'b0;
          rempty_out = 1'b0;
        end
      end
      default: ;
    endcase
  end
  
  // synopsys translate_off
  _auto_legal_state: assert property (@(posedge clk) rst_n |-> state_r < 3)
    else $fatal(1, "FSM ILLEGAL STATE: Bitstream.state_r = %0d", state_r);
  _auto_reach_Idle: cover property (@(posedge clk) state_r == IDLE);
  _auto_reach_WaitR: cover property (@(posedge clk) state_r == WAITR);
  _auto_reach_Ready: cover property (@(posedge clk) state_r == READY);
  _auto_tr_IDLE_to_WAITR: cover property (@(posedge clk) state_r == IDLE && state_next == WAITR);
  _auto_tr_WAITR_to_READY: cover property (@(posedge clk) state_r == WAITR && state_next == READY);
  _auto_tr_READY_to_WAITR: cover property (@(posedge clk) state_r == READY && state_next == WAITR);
  // synopsys translate_on

endmodule

