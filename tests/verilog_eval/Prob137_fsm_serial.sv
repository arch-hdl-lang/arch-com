Wrote tests/verilog_eval/Prob137_fsm_serial.sv
ogic reset,
  input logic in,
  output logic done
);

  typedef enum logic [3:0] {
    BIT0 = 4'd0,
    BIT1 = 4'd1,
    BIT2 = 4'd2,
    BIT3 = 4'd3,
    BIT4 = 4'd4,
    BIT5 = 4'd5,
    BIT6 = 4'd6,
    BIT7 = 4'd7,
    IDLE = 4'd8,
    STOP = 4'd9,
    OK = 4'd10,
    ERR = 4'd11
  } TopModule_state_t;
  
  TopModule_state_t state_r, state_next;
  
  always_ff @(posedge clk) begin
    if (reset) begin
      state_r <= IDLE;
    end else begin
      state_r <= state_next;
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      IDLE: begin
        if (~in) state_next = BIT0;
      end
      BIT0: begin
        state_next = BIT1;
      end
      BIT1: begin
        state_next = BIT2;
      end
      BIT2: begin
        state_next = BIT3;
      end
      BIT3: begin
        state_next = BIT4;
      end
      BIT4: begin
        state_next = BIT5;
      end
      BIT5: begin
        state_next = BIT6;
      end
      BIT6: begin
        state_next = BIT7;
      end
      BIT7: begin
        state_next = STOP;
      end
      STOP: begin
        if (in) state_next = OK;
        else if (~in) state_next = ERR;
      end
      OK: begin
        if (in) state_next = IDLE;
        else if (~in) state_next = BIT0;
      end
      ERR: begin
        if (in) state_next = IDLE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    done = 1'b0;
    case (state_r)
      IDLE: begin
      end
      BIT0: begin
      end
      BIT1: begin
      end
      BIT2: begin
      end
      BIT3: begin
      end
      BIT4: begin
      end
      BIT5: begin
      end
      BIT6: begin
      end
      BIT7: begin
      end
      STOP: begin
      end
      OK: begin
        done = 1'b1;
      end
      ERR: begin
      end
      default: ;
    endcase
  end

endmodule

