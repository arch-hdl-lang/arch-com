Wrote tests/verilog_eval/Prob155_lemmings4.sv
/splatter FSM with async reset
// Fall >= 20 cycles then hit ground = splat (dead forever)
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
    DIGRIGHT = 3'd5,
    SPLAT = 3'd6
  } TopModule_state_t;
  
  TopModule_state_t state_r, state_next;
  
  logic [5-1:0] fall_count;
  
  always_ff @(posedge clk or posedge areset) begin
    if (areset) begin
      state_r <= WALKLEFT;
      fall_count <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        WALKLEFT: begin
          fall_count <= 0;
        end
        WALKRIGHT: begin
          fall_count <= 0;
        end
        FALLLEFT: begin
          fall_count <= fall_count < 20 ? 5'(fall_count + 1) : fall_count;
        end
        FALLRIGHT: begin
          fall_count <= fall_count < 20 ? 5'(fall_count + 1) : fall_count;
        end
        DIGLEFT: begin
          fall_count <= 0;
        end
        DIGRIGHT: begin
          fall_count <= 0;
        end
        SPLAT: begin
          fall_count <= 0;
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      WALKLEFT: begin
        if (~ground) state_next = FALLLEFT;
        else if (ground & dig) state_next = DIGLEFT;
        else if (ground & ~dig & bump_left) state_next = WALKRIGHT;
      end
      WALKRIGHT: begin
        if (~ground) state_next = FALLRIGHT;
        else if (ground & dig) state_next = DIGRIGHT;
        else if (ground & ~dig & bump_right) state_next = WALKLEFT;
      end
      FALLLEFT: begin
        if (ground & fall_count >= 20) state_next = SPLAT;
        else if (ground & fall_count < 20) state_next = WALKLEFT;
      end
      FALLRIGHT: begin
        if (ground & fall_count >= 20) state_next = SPLAT;
        else if (ground & fall_count < 20) state_next = WALKRIGHT;
      end
      DIGLEFT: begin
        if (~ground) state_next = FALLLEFT;
      end
      DIGRIGHT: begin
        if (~ground) state_next = FALLRIGHT;
      end
      SPLAT: begin
        if (ground) state_next = SPLAT;
        else if (~ground) state_next = SPLAT;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    walk_left = 1'b0;
    walk_right = 1'b0;
    aaah = 1'b0;
    digging = 1'b0;
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
      SPLAT: begin
      end
      default: ;
    endcase
  end

endmodule

