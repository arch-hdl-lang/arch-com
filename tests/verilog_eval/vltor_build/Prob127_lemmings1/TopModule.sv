// domain SysDomain

module TopModule (
  input logic clk,
  input logic areset,
  input logic bump_left,
  input logic bump_right,
  output logic walk_left,
  output logic walk_right
);

  typedef enum logic [0:0] {
    WALKLEFT = 1'd0,
    WALKRIGHT = 1'd1
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
        if (bump_left) state_next = WALKRIGHT;
      end
      WALKRIGHT: begin
        if (bump_right) state_next = WALKLEFT;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    case (state_r)
      WALKLEFT: begin
        walk_left = 1'b1;
        walk_right = 1'b0;
      end
      WALKRIGHT: begin
        walk_left = 1'b0;
        walk_right = 1'b1;
      end
      default: ;
    endcase
  end

endmodule

