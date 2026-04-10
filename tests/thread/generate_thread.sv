module _MultiChannel_Channel_0 (
  input logic clk,
  input logic rst_n,
  input logic ready_0,
  output logic valid_0
);

  typedef enum logic [0:0] {
    S0 = 1'd0
  } _MultiChannel_Channel_0_state_t;
  
  _MultiChannel_Channel_0_state_t state_r, state_next;
  
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
        if (ready_0) state_next = S0;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    valid_0 = 0;
    case (state_r)
      S0: begin
        valid_0 = 1;
      end
      default: ;
    endcase
  end

endmodule

module _MultiChannel_Channel_1 (
  input logic clk,
  input logic rst_n,
  input logic ready_1,
  output logic valid_1
);

  typedef enum logic [0:0] {
    S0 = 1'd0
  } _MultiChannel_Channel_1_state_t;
  
  _MultiChannel_Channel_1_state_t state_r, state_next;
  
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
        if (ready_1) state_next = S0;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    valid_1 = 0;
    case (state_r)
      S0: begin
        valid_1 = 1;
      end
      default: ;
    endcase
  end

endmodule

module _MultiChannel_Channel_2 (
  input logic clk,
  input logic rst_n,
  input logic ready_2,
  output logic valid_2
);

  typedef enum logic [0:0] {
    S0 = 1'd0
  } _MultiChannel_Channel_2_state_t;
  
  _MultiChannel_Channel_2_state_t state_r, state_next;
  
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
        if (ready_2) state_next = S0;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    valid_2 = 0;
    case (state_r)
      S0: begin
        valid_2 = 1;
      end
      default: ;
    endcase
  end

endmodule

module _MultiChannel_Channel_3 (
  input logic clk,
  input logic rst_n,
  input logic ready_3,
  output logic valid_3
);

  typedef enum logic [0:0] {
    S0 = 1'd0
  } _MultiChannel_Channel_3_state_t;
  
  _MultiChannel_Channel_3_state_t state_r, state_next;
  
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
        if (ready_3) state_next = S0;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    valid_3 = 0;
    case (state_r)
      S0: begin
        valid_3 = 1;
      end
      default: ;
    endcase
  end

endmodule

module MultiChannel #(
  parameter int NUM_CH = 4
) (
  input logic clk,
  input logic rst_n,
  output logic valid_0,
  output logic valid_1,
  output logic valid_2,
  output logic valid_3,
  input logic ready_0,
  input logic ready_1,
  input logic ready_2,
  input logic ready_3
);

  _MultiChannel_Channel_0 _Channel_0 (
    .clk(clk),
    .rst_n(rst_n),
    .ready_0(ready_0),
    .valid_0(valid_0)
  );
  _MultiChannel_Channel_1 _Channel_1 (
    .clk(clk),
    .rst_n(rst_n),
    .ready_1(ready_1),
    .valid_1(valid_1)
  );
  _MultiChannel_Channel_2 _Channel_2 (
    .clk(clk),
    .rst_n(rst_n),
    .ready_2(ready_2),
    .valid_2(valid_2)
  );
  _MultiChannel_Channel_3 _Channel_3 (
    .clk(clk),
    .rst_n(rst_n),
    .ready_3(ready_3),
    .valid_3(valid_3)
  );

endmodule

