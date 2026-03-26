Wrote tests/verilog_eval/Prob139_2013_q2bfsm.sv
tive-low sync reset (resetn)
// States: A(reset), SetF, X1, X0, X01, SetG1, SetG2, GoodG, BadG
module TopModule (
  input logic clk,
  input logic resetn,
  input logic x,
  input logic y,
  output logic f,
  output logic g
);

  typedef enum logic [3:0] {
    A = 4'd0,
    SETF = 4'd1,
    X1 = 4'd2,
    X0 = 4'd3,
    X01 = 4'd4,
    SETG1 = 4'd5,
    SETG2 = 4'd6,
    GOODG = 4'd7,
    BADG = 4'd8
  } TopModule_state_t;
  
  TopModule_state_t state_r, state_next;
  
  always_ff @(posedge clk) begin
    if ((!resetn)) begin
      state_r <= A;
    end else begin
      state_r <= state_next;
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      A: begin
        state_next = SETF;
      end
      SETF: begin
        state_next = X1;
      end
      X1: begin
        if (x) state_next = X0;
      end
      X0: begin
        if (~x) state_next = X01;
      end
      X01: begin
        if (x) state_next = SETG1;
        else if (~x) state_next = X1;
      end
      SETG1: begin
        if (y) state_next = GOODG;
        else if (~y) state_next = SETG2;
      end
      SETG2: begin
        if (y) state_next = GOODG;
        else if (~y) state_next = BADG;
      end
      GOODG: begin
        state_next = GOODG;
      end
      BADG: begin
        state_next = BADG;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    f = 1'b0;
    g = 1'b0;
    case (state_r)
      A: begin
      end
      SETF: begin
        f = 1'b1;
      end
      X1: begin
      end
      X0: begin
      end
      X01: begin
      end
      SETG1: begin
        g = 1'b1;
      end
      SETG2: begin
        g = 1'b1;
      end
      GOODG: begin
        g = 1'b1;
      end
      BADG: begin
      end
      default: ;
    endcase
  end

endmodule

