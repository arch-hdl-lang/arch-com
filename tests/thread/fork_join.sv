module _AxiWrite_thread (
  input logic clk,
  input logic rst_n,
  input logic aw_ready,
  input logic b_valid,
  input logic w_ready,
  output logic [32-1:0] aw_addr,
  output logic aw_valid,
  output logic b_ready,
  output logic [32-1:0] w_data,
  output logic w_valid
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
    S8 = 4'd8,
    S9 = 4'd9
  } _AxiWrite_thread_state_t;
  
  _AxiWrite_thread_state_t state_r, state_next;
  
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      state_r <= S0;
    end else begin
      state_r <= state_next;
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      S0: begin
        if (aw_ready && w_ready) state_next = S4;
        else if (w_ready && !aw_ready) state_next = S3;
        else if (aw_ready && !w_ready) state_next = S1;
      end
      S1: begin
        if (w_ready) state_next = S5;
        else if (1'b1 && !w_ready) state_next = S2;
      end
      S2: begin
        if (w_ready) state_next = S5;
      end
      S3: begin
        if (aw_ready) state_next = S7;
        else if (1'b1 && !aw_ready) state_next = S6;
      end
      S4: begin
        state_next = S8;
      end
      S5: begin
        state_next = S8;
      end
      S6: begin
        if (aw_ready) state_next = S7;
      end
      S7: begin
        state_next = S8;
      end
      S8: begin
        state_next = S9;
      end
      S9: begin
        if (b_valid) state_next = S0;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    aw_addr = 0;
    aw_valid = 0;
    b_ready = 0;
    w_data = 0;
    w_valid = 0;
    case (state_r)
      S0: begin
        aw_valid = 1;
        aw_addr = 32'd40960;
        w_valid = 1;
        w_data = 32'd57005;
      end
      S1: begin
        aw_valid = 0;
        w_valid = 1;
        w_data = 32'd57005;
      end
      S2: begin
        w_valid = 1;
        w_data = 32'd57005;
      end
      S3: begin
        aw_valid = 1;
        aw_addr = 32'd40960;
        w_valid = 0;
      end
      S4: begin
        aw_valid = 0;
        w_valid = 0;
      end
      S5: begin
        w_valid = 0;
      end
      S6: begin
        aw_valid = 1;
        aw_addr = 32'd40960;
      end
      S7: begin
        aw_valid = 0;
      end
      S8: begin
      end
      S9: begin
        b_ready = 1;
      end
      default: ;
    endcase
  end

endmodule

module AxiWrite (
  input logic clk,
  input logic rst_n,
  output logic aw_valid,
  output logic [32-1:0] aw_addr,
  input logic aw_ready,
  output logic w_valid,
  output logic [32-1:0] w_data,
  input logic w_ready,
  output logic b_ready,
  input logic b_valid
);

  _AxiWrite_thread _thread (
    .clk(clk),
    .rst_n(rst_n),
    .aw_ready(aw_ready),
    .b_valid(b_valid),
    .w_ready(w_ready),
    .aw_addr(aw_addr),
    .aw_valid(aw_valid),
    .b_ready(b_ready),
    .w_data(w_data),
    .w_valid(w_valid)
  );

endmodule

