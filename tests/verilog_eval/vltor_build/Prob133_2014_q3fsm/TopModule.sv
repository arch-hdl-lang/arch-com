// VerilogEval Prob133: FSM waits for s=1, then counts w over 3-cycle windows
// domain SysDomain

module TopModule (
  input logic clk,
  input logic reset,
  input logic s,
  input logic w,
  output logic z
);

  typedef enum logic [2:0] {
    A = 3'd0,
    B1 = 3'd1,
    B2 = 3'd2,
    B3 = 3'd3,
    OUT = 3'd4
  } TopModule_state_t;
  
  TopModule_state_t state_r, state_next;
  
  logic [2-1:0] cnt;
  
  always_ff @(posedge clk) begin
    if (reset) begin
      state_r <= A;
      cnt <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        A: begin
          cnt <= 0;
        end
        B1: begin
          if (w) begin
            cnt <= 1;
          end else begin
            cnt <= 0;
          end
        end
        B2: begin
          if (w) begin
            cnt <= 2'(cnt + 1);
          end
        end
        B3: begin
          if (w) begin
            cnt <= 2'(cnt + 1);
          end
        end
        OUT: begin
          if (w) begin
            cnt <= 1;
          end else begin
            cnt <= 0;
          end
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      A: begin
        if (s) state_next = B1;
      end
      B1: begin
        state_next = B2;
      end
      B2: begin
        state_next = B3;
      end
      B3: begin
        state_next = OUT;
      end
      OUT: begin
        state_next = B2;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    case (state_r)
      A: begin
        z = 1'b0;
      end
      B1: begin
        z = 1'b0;
      end
      B2: begin
        z = 1'b0;
      end
      B3: begin
        z = 1'b0;
      end
      OUT: begin
        z = 1'b0;
        if (cnt == 2) begin
          z = 1'b1;
        end
      end
      default: ;
    endcase
  end

endmodule

