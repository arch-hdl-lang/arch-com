// VerilogEval Prob140: HDLC framing FSM - disc, flag, err detection
// domain SysDomain

module TopModule (
  input logic clk,
  input logic reset,
  input logic in,
  output logic disc,
  output logic flag,
  output logic err
);

  typedef enum logic [3:0] {
    S0 = 4'd0,
    S1 = 4'd1,
    S2 = 4'd2,
    S3 = 4'd3,
    S4 = 4'd4,
    S5 = 4'd5,
    S6 = 4'd6,
    SERR = 4'd7,
    SDISC = 4'd8,
    SFLAG = 4'd9
  } TopModule_state_t;
  
  TopModule_state_t state_r, state_next;
  
  always_ff @(posedge clk) begin
    if (reset) begin
      state_r <= S0;
    end else begin
      state_r <= state_next;
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      S0: begin
        if (in) state_next = S1;
      end
      S1: begin
        if (in) state_next = S2;
        else if ((~in)) state_next = S0;
      end
      S2: begin
        if (in) state_next = S3;
        else if ((~in)) state_next = S0;
      end
      S3: begin
        if (in) state_next = S4;
        else if ((~in)) state_next = S0;
      end
      S4: begin
        if (in) state_next = S5;
        else if ((~in)) state_next = S0;
      end
      S5: begin
        if (in) state_next = S6;
        else if ((~in)) state_next = SDISC;
      end
      S6: begin
        if (in) state_next = SERR;
        else if ((~in)) state_next = SFLAG;
      end
      SERR: begin
        if (in) state_next = SERR;
        else if ((~in)) state_next = S0;
      end
      SDISC: begin
        if (in) state_next = S1;
        else if ((~in)) state_next = S0;
      end
      SFLAG: begin
        if (in) state_next = S1;
        else if ((~in)) state_next = S0;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    disc = 1'b0; // default
    flag = 1'b0; // default
    err = 1'b0; // default
    case (state_r)
      S0: begin
      end
      S1: begin
      end
      S2: begin
      end
      S3: begin
      end
      S4: begin
      end
      S5: begin
      end
      S6: begin
      end
      SERR: begin
        err = 1'b1;
      end
      SDISC: begin
        disc = 1'b1;
      end
      SFLAG: begin
        flag = 1'b1;
      end
      default: ;
    endcase
  end

endmodule

