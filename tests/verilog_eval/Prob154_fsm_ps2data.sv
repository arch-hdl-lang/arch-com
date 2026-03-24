// VerilogEval Prob154: PS/2 3-byte message FSM with data output
// Find byte with in[3]=1, collect 3 bytes, assert done
// domain SysDomain

module TopModule (
  input logic clk,
  input logic reset,
  input logic [8-1:0] in,
  output logic [24-1:0] out_bytes,
  output logic done
);

  typedef enum logic [1:0] {
    FIND = 2'd0,
    GOT1 = 2'd1,
    GOT2 = 2'd2,
    DONE_ST = 2'd3
  } TopModule_state_t;
  
  TopModule_state_t state_r, state_next;
  
  logic [24-1:0] out_r;
  
  always_ff @(posedge clk) begin
    if (reset) begin
      state_r <= FIND;
      out_r <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        FIND: begin
          for (int i = 0; i <= 15; i++) begin
            out_r[(i + 8)] <= out_r[i];
          end
          for (int i = 0; i <= 7; i++) begin
            out_r[i] <= in[i];
          end
        end
        GOT1: begin
          for (int i = 0; i <= 15; i++) begin
            out_r[(i + 8)] <= out_r[i];
          end
          for (int i = 0; i <= 7; i++) begin
            out_r[i] <= in[i];
          end
        end
        GOT2: begin
          for (int i = 0; i <= 15; i++) begin
            out_r[(i + 8)] <= out_r[i];
          end
          for (int i = 0; i <= 7; i++) begin
            out_r[i] <= in[i];
          end
        end
        DONE_ST: begin
          for (int i = 0; i <= 15; i++) begin
            out_r[(i + 8)] <= out_r[i];
          end
          for (int i = 0; i <= 7; i++) begin
            out_r[i] <= in[i];
          end
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      FIND: begin
        if (in[3]) state_next = GOT1;
      end
      GOT1: begin
        state_next = GOT2;
      end
      GOT2: begin
        state_next = DONE_ST;
      end
      DONE_ST: begin
        if (in[3]) state_next = GOT1;
        else if ((~in[3])) state_next = FIND;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    out_bytes = 0; // default
    done = 1'b0; // default
    case (state_r)
      FIND: begin
      end
      GOT1: begin
      end
      GOT2: begin
      end
      DONE_ST: begin
        done = 1'b1;
        for (int i = 0; i <= 23; i++) begin
          out_bytes[i] = out_r[i];
        end
      end
      default: ;
    endcase
  end

endmodule

