module _MultiReady_Req_0 (
  input logic clk,
  input logic rst_n,
  input logic [2-1:0] r_id,
  input logic r_valid,
  output logic r_ready
);

  typedef enum logic [0:0] {
    S0 = 1'd0
  } _MultiReady_Req_0_state_t;
  
  _MultiReady_Req_0_state_t state_r, state_next;
  
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
        if (r_valid && r_id == 2'd0) state_next = S0;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    r_ready = 0;
    case (state_r)
      S0: begin
        r_ready = 1;
      end
      default: ;
    endcase
  end

endmodule

module _MultiReady_Req_1 (
  input logic clk,
  input logic rst_n,
  input logic [2-1:0] r_id,
  input logic r_valid,
  output logic r_ready
);

  typedef enum logic [0:0] {
    S0 = 1'd0
  } _MultiReady_Req_1_state_t;
  
  _MultiReady_Req_1_state_t state_r, state_next;
  
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
        if (r_valid && r_id == 2'd1) state_next = S0;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    r_ready = 0;
    case (state_r)
      S0: begin
        r_ready = 1;
      end
      default: ;
    endcase
  end

endmodule

module MultiReady (
  input logic clk,
  input logic rst_n,
  output logic r_ready,
  input logic r_valid,
  input logic [2-1:0] r_id
);

  logic r_ready__t1;
  logic r_ready__t0;
  logic [32-1:0] buf_0;
  logic [32-1:0] buf_1;
  _MultiReady_Req_0 _Req_0 (
    .clk(clk),
    .rst_n(rst_n),
    .r_id(r_id),
    .r_valid(r_valid),
    .r_ready(r_ready__t0)
  );
  _MultiReady_Req_1 _Req_1 (
    .clk(clk),
    .rst_n(rst_n),
    .r_id(r_id),
    .r_valid(r_valid),
    .r_ready(r_ready__t1)
  );
  assign r_ready = r_ready__t0 | r_ready__t1;

endmodule

