module findfasterclock (
  input logic clk_A,
  input logic clk_B,
  input logic rst_n,
  output logic A_faster_than_B,
  output logic valid
);

  // State encoding
  logic [2-1:0] ST_IDLE;
  assign ST_IDLE = 0;
  logic [2-1:0] ST_COUNT;
  assign ST_COUNT = 1;
  logic [2-1:0] ST_DONE;
  assign ST_DONE = 2;
  // A-domain FSM registers
  logic [2-1:0] stateA;
  logic measure_B;
  logic [32-1:0] periodA;
  logic done_A;
  // B-domain FSM registers
  logic [2-1:0] stateB;
  logic measure_A;
  logic [32-1:0] periodB;
  logic done_B;
  // Cross-domain counters
  logic [32-1:0] b_count;
  logic [32-1:0] a_count;
  // A-domain FSM: measures periodA by counting B pulses during one A cycle
  always_ff @(posedge clk_A or negedge rst_n) begin
    if ((!rst_n)) begin
      done_A <= 1'b0;
      measure_B <= 1'b0;
      periodA <= 0;
      stateA <= 0;
    end else begin
      if (stateA == ST_IDLE) begin
        measure_B <= 1'b1;
        stateA <= ST_COUNT;
      end else if (stateA == ST_COUNT) begin
        periodA <= b_count;
        measure_B <= 1'b0;
        done_A <= 1'b1;
        stateA <= ST_DONE;
      end else begin
        stateA <= ST_DONE;
      end
    end
  end
  // B-domain FSM: measures periodB by counting A pulses during one B cycle
  always_ff @(posedge clk_B or negedge rst_n) begin
    if ((!rst_n)) begin
      done_B <= 1'b0;
      measure_A <= 1'b0;
      periodB <= 0;
      stateB <= 0;
    end else begin
      if (stateB == ST_IDLE) begin
        measure_A <= 1'b1;
        stateB <= ST_COUNT;
      end else if (stateB == ST_COUNT) begin
        periodB <= a_count;
        measure_A <= 1'b0;
        done_B <= 1'b1;
        stateB <= ST_DONE;
      end else begin
        stateB <= ST_DONE;
      end
    end
  end
  // B-domain counter: count B rising edges while measure_B is asserted
  always_ff @(posedge clk_B or negedge rst_n) begin
    if ((!rst_n)) begin
      b_count <= 0;
    end else begin
      if (measure_B) begin
        b_count <= 32'(b_count + 1);
      end else begin
        b_count <= 0;
      end
    end
  end
  // A-domain counter: count A rising edges while measure_A is asserted
  always_ff @(posedge clk_A or negedge rst_n) begin
    if ((!rst_n)) begin
      a_count <= 0;
    end else begin
      if (measure_A) begin
        a_count <= 32'(a_count + 1);
      end else begin
        a_count <= 0;
      end
    end
  end
  // Combinational output logic
  always_comb begin
    if (~rst_n) begin
      valid = 1'b0;
      A_faster_than_B = 1'b0;
    end else begin
      valid = done_A & done_B;
      if (done_A & done_B) begin
        A_faster_than_B = periodB > periodA;
      end else begin
        A_faster_than_B = 1'b0;
      end
    end
  end

endmodule

