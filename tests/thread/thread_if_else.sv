module _ThreadIfElse_thread (
  input logic clk,
  input logic rst_n,
  input logic mode,
  input logic ready,
  output logic [32-1:0] addr,
  output logic valid
);

  typedef enum logic [0:0] {
    S0 = 1'd0
  } _ThreadIfElse_thread_state_t;
  
  _ThreadIfElse_thread_state_t state_r, state_next;
  
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
        if (ready) state_next = S0;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    addr = 0;
    valid = 0;
    case (state_r)
      S0: begin
        valid = 1;
        if (mode) begin
          addr = 32'd1000;
        end else begin
          addr = 32'd2000;
        end
      end
      default: ;
    endcase
  end

endmodule

module ThreadIfElse (
  input logic clk,
  input logic rst_n,
  input logic mode,
  output logic [32-1:0] addr,
  output logic valid,
  input logic ready
);

  _ThreadIfElse_thread _thread (
    .clk(clk),
    .rst_n(rst_n),
    .mode(mode),
    .ready(ready),
    .addr(addr),
    .valid(valid)
  );

endmodule

