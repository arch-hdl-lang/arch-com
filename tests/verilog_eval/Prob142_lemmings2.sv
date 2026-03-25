// VerilogEval Prob142: Lemmings walk/fall FSM with async reset
// domain SysDomain

module TopModule (
  input logic clk,
  input logic areset,
  input logic bump_left,
  input logic bump_right,
  input logic ground,
  output logic walk_left,
  output logic walk_right,
  output logic aaah
);

  typedef enum logic [1:0] {
    WALKLEFT = 2'd0,
    WALKRIGHT = 2'd1,
    FALLLEFT = 2'd2,
    FALLRIGHT = 2'd3
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
        if (~ground) state_next = FALLLEFT;
        else if (ground & bump_left) state_next = WALKRIGHT;
      end
      WALKRIGHT: begin
        if (~ground) state_next = FALLRIGHT;
        else if (ground & bump_right) state_next = WALKLEFT;
      end
      FALLLEFT: begin
        if (ground) state_next = WALKLEFT;
      end
      FALLRIGHT: begin
        if (ground) state_next = WALKRIGHT;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    walk_left = 1'b0;
    walk_right = 1'b0;
    aaah = 1'b0;
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
      default: ;
    endcase
  end

endmodule

