// VerilogEval Prob152: Lemmings walk/fall/dig FSM with async reset
// domain SysDomain

module TopModule (
  input logic clk,
  input logic areset,
  input logic bump_left,
  input logic bump_right,
  input logic ground,
  input logic dig,
  output logic walk_left,
  output logic walk_right,
  output logic aaah,
  output logic digging
);

  typedef enum logic [2:0] {
    WALKLEFT = 3'd0,
    WALKRIGHT = 3'd1,
    FALLLEFT = 3'd2,
    FALLRIGHT = 3'd3,
    DIGLEFT = 3'd4,
    DIGRIGHT = 3'd5
  } TopModule_state_t;
  
  TopModule_state_t state_r, state_next;
  
  always_ff @(posedge clk or posedge areset) begin
    if (areset) begin
      state_r <= WALKLEFT;
    end else begin
      state_r <= state_next;
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      WALKLEFT: begin
        if ((~ground)) state_next = FALLLEFT;
        else if ((ground & dig)) state_next = DIGLEFT;
        else if (((ground & (~dig)) & bump_left)) state_next = WALKRIGHT;
      end
      WALKRIGHT: begin
        if ((~ground)) state_next = FALLRIGHT;
        else if ((ground & dig)) state_next = DIGRIGHT;
        else if (((ground & (~dig)) & bump_right)) state_next = WALKLEFT;
      end
      FALLLEFT: begin
        if (ground) state_next = WALKLEFT;
      end
      FALLRIGHT: begin
        if (ground) state_next = WALKRIGHT;
      end
      DIGLEFT: begin
        if ((~ground)) state_next = FALLLEFT;
      end
      DIGRIGHT: begin
        if ((~ground)) state_next = FALLRIGHT;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    walk_left = 1'b0; // default
    walk_right = 1'b0; // default
    aaah = 1'b0; // default
    digging = 1'b0; // default
    case (state_r)
      WALKLEFT: begin
        walk_left = 1'b1;
      end
      WALKRIGHT: begin
        walk_right = 1'b1;
      end
      FALLLEFT: begin
        aaah = 1'b1;
      end
      FALLRIGHT: begin
        aaah = 1'b1;
      end
      DIGLEFT: begin
        digging = 1'b1;
      end
      DIGRIGHT: begin
        digging = 1'b1;
      end
      default: ;
    endcase
  end

endmodule

