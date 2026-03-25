// VerilogEval Prob128: PS/2 mouse 3-byte message boundary detection
// domain SysDomain

module TopModule (
  input logic clk,
  input logic reset,
  input logic [8-1:0] in,
  output logic done
);

  typedef enum logic [1:0] {
    BYTE1 = 2'd0,
    BYTE2 = 2'd1,
    BYTE3 = 2'd2,
    DONES = 2'd3
  } TopModule_state_t;
  
  TopModule_state_t state_r, state_next;
  
  always_ff @(posedge clk) begin
    if (reset) begin
      state_r <= BYTE1;
    end else begin
      state_r <= state_next;
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      BYTE1: begin
        if (in[3]) state_next = BYTE2;
      end
      BYTE2: begin
        state_next = BYTE3;
      end
      BYTE3: begin
        state_next = DONES;
      end
      DONES: begin
        if (in[3]) state_next = BYTE2;
        else if (~in[3]) state_next = BYTE1;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    case (state_r)
      BYTE1: begin
        done = 1'b0;
      end
      BYTE2: begin
        done = 1'b0;
      end
      BYTE3: begin
        done = 1'b0;
      end
      DONES: begin
        done = 1'b1;
      end
      default: ;
    endcase
  end

endmodule

