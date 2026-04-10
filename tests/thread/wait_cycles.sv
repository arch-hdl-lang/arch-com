module _DelayPulse_thread (
  input logic clk,
  input logic rst_n,
  input logic start,
  output logic pulse
);

  typedef enum logic [1:0] {
    S0 = 2'd0,
    S1 = 2'd1,
    S2 = 2'd2
  } _DelayPulse_thread_state_t;
  
  _DelayPulse_thread_state_t state_r, state_next;
  
  logic [32-1:0] _cnt;
  
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      state_r <= S0;
      _cnt <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        S0: begin
          if (start) begin
            _cnt <= 5 - 1;
          end
        end
        S1: begin
          _cnt <= _cnt - 1;
          if (_cnt == 0) begin
            _cnt <= 1 - 1;
          end
        end
        S2: begin
          _cnt <= _cnt - 1;
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      S0: begin
        if (start) state_next = S1;
      end
      S1: begin
        if (_cnt == 0) state_next = S2;
      end
      S2: begin
        if (_cnt == 0) state_next = S0;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    pulse = 0;
    case (state_r)
      S0: begin
      end
      S1: begin
      end
      S2: begin
        pulse = 1;
      end
      default: ;
    endcase
  end

endmodule

module DelayPulse (
  input logic clk,
  input logic rst_n,
  input logic start,
  output logic pulse
);

  _DelayPulse_thread _thread (
    .clk(clk),
    .rst_n(rst_n),
    .start(start),
    .pulse(pulse)
  );

endmodule

