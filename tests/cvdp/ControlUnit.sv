module ControlUnit (
  input logic clk,
  input logic rst,
  input logic hit,
  input logic miss,
  input logic ready,
  output logic tlb_write_enable,
  output logic flsh
);

  typedef enum logic [1:0] {
    IDLE = 2'd0,
    FETCH = 2'd1,
    UPDATE = 2'd2
  } ControlUnit_state_t;
  
  ControlUnit_state_t state_r, state_next;
  
  always_ff @(posedge clk) begin
    if (rst) begin
      state_r <= IDLE;
    end else begin
      state_r <= state_next;
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      IDLE: begin
        if (miss) state_next = FETCH;
      end
      FETCH: begin
        if (ready) state_next = UPDATE;
      end
      UPDATE: begin
        state_next = IDLE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    tlb_write_enable = 1'b0;
    flsh = 1'b0;
    case (state_r)
      IDLE: begin
      end
      FETCH: begin
      end
      UPDATE: begin
        tlb_write_enable = 1'b1;
      end
      default: ;
    endcase
  end

endmodule

