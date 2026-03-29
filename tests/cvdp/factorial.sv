module factorial (
  input logic clk,
  input logic arst_n,
  input logic [5-1:0] num_in,
  input logic start,
  output logic busy,
  output logic done,
  output logic [64-1:0] fact
);

  typedef enum logic [1:0] {
    IDLE = 2'd0,
    BUSY = 2'd1,
    DONE = 2'd2
  } factorial_state_t;
  
  factorial_state_t state_r, state_next;
  
  logic [5-1:0] cnt;
  logic [64-1:0] acc;
  
  always_ff @(posedge clk or negedge arst_n) begin
    if ((!arst_n)) begin
      state_r <= IDLE;
      cnt <= 0;
      acc <= 1;
    end else begin
      state_r <= state_next;
      case (state_r)
        IDLE: begin
          if (start) begin
            cnt <= num_in;
            acc <= 1;
            busy <= 1'b1;
            done <= 1'b0;
            fact <= 0;
          end
        end
        BUSY: begin
          if (cnt < 2) begin
            done <= 1'b1;
            busy <= 1'b0;
            fact <= acc;
            cnt <= 0;
          end else begin
            acc <= acc * 64'($unsigned(cnt));
            cnt <= cnt - 1;
          end
        end
        DONE: begin
          done <= 1'b0;
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      IDLE: begin
        if (start) state_next = BUSY;
      end
      BUSY: begin
        if (cnt < 2) state_next = DONE;
      end
      DONE: begin
        if (done) state_next = IDLE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    case (state_r)
      IDLE: begin
      end
      BUSY: begin
      end
      DONE: begin
      end
      default: ;
    endcase
  end

endmodule

