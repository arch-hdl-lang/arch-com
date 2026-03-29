module fsm_seq_detector (
  input logic clk_in,
  input logic rst_in,
  input logic seq_in,
  output logic seq_detected
);

  typedef enum logic [3:0] {
    S0 = 4'd0,
    S1 = 4'd1,
    S2 = 4'd2,
    S3 = 4'd3,
    S4 = 4'd4,
    S5 = 4'd5,
    S6 = 4'd6,
    S7 = 4'd7,
    S8 = 4'd8
  } fsm_seq_detector_state_t;
  
  fsm_seq_detector_state_t state_r, state_next;
  
  always_ff @(posedge clk_in or posedge rst_in) begin
    if (rst_in) begin
      state_r <= S0;
    end else begin
      state_r <= state_next;
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      S0: begin
        if (seq_in) state_next = S1;
      end
      S1: begin
        if (~seq_in) state_next = S2;
        else if (seq_in) state_next = S1;
      end
      S2: begin
        if (seq_in) state_next = S3;
        else if (~seq_in) state_next = S0;
      end
      S3: begin
        if (seq_in) state_next = S4;
        else if (~seq_in) state_next = S2;
      end
      S4: begin
        if (~seq_in) state_next = S5;
        else if (seq_in) state_next = S1;
      end
      S5: begin
        if (seq_in) state_next = S3;
        else if (~seq_in) state_next = S6;
      end
      S6: begin
        if (seq_in) state_next = S1;
        else if (~seq_in) state_next = S7;
      end
      S7: begin
        if (seq_in) state_next = S8;
        else if (~seq_in) state_next = S0;
      end
      S8: begin
        if (seq_in) state_next = S1;
        else if (~seq_in) state_next = S2;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    seq_detected = 1'b0;
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
      S7: begin
      end
      S8: begin
        seq_detected = 1'b1;
      end
      default: ;
    endcase
  end

endmodule

