// VerilogEval Prob151: Timer FSM (registered version) - detect 1101, shift 4, count, wait ack
// domain SysDomain

module TopModule (
  input logic clk,
  input logic reset_sig,
  input logic data,
  input logic done_counting,
  input logic ack,
  output logic shift_ena,
  output logic counting,
  output logic done
);

  typedef enum logic [3:0] {
    S = 4'd0,
    S1 = 4'd1,
    S11 = 4'd2,
    S110 = 4'd3,
    B0 = 4'd4,
    B1 = 4'd5,
    B2 = 4'd6,
    B3 = 4'd7,
    COUNT = 4'd8,
    WAIT = 4'd9
  } TopModule_state_t;
  
  TopModule_state_t state_r, state_next;
  
  always_ff @(posedge clk) begin
    if (reset_sig) begin
      state_r <= S;
    end else begin
      state_r <= state_next;
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      S: begin
        if (data) state_next = S1;
      end
      S1: begin
        if (data) state_next = S11;
        else if ((~data)) state_next = S;
      end
      S11: begin
        if (data) state_next = S11;
        else if ((~data)) state_next = S110;
      end
      S110: begin
        if (data) state_next = B0;
        else if ((~data)) state_next = S;
      end
      B0: begin
        state_next = B1;
      end
      B1: begin
        state_next = B2;
      end
      B2: begin
        state_next = B3;
      end
      B3: begin
        state_next = COUNT;
      end
      COUNT: begin
        if (done_counting) state_next = WAIT;
        else if ((~done_counting)) state_next = COUNT;
      end
      WAIT: begin
        if (ack) state_next = S;
        else if ((~ack)) state_next = WAIT;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    shift_ena = 1'b0; // default
    counting = 1'b0; // default
    done = 1'b0; // default
    case (state_r)
      S: begin
      end
      S1: begin
      end
      S11: begin
      end
      S110: begin
      end
      B0: begin
        shift_ena = 1'b1;
      end
      B1: begin
        shift_ena = 1'b1;
      end
      B2: begin
        shift_ena = 1'b1;
      end
      B3: begin
        shift_ena = 1'b1;
      end
      COUNT: begin
        counting = 1'b1;
      end
      WAIT: begin
        done = 1'b1;
      end
      default: ;
    endcase
  end

endmodule

