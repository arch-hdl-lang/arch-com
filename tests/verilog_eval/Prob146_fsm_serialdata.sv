// VerilogEval Prob146: Serial receiver with data output
// Start bit (0), 8 data bits LSB-first, stop bit (1). Assert done when stop=1.
// domain SysDomain

module TopModule (
  input logic clk,
  input logic reset,
  input logic in,
  output logic [8-1:0] out_byte,
  output logic done
);

  typedef enum logic [3:0] {
    BIT0 = 4'd0,
    BIT1 = 4'd1,
    BIT2 = 4'd2,
    BIT3 = 4'd3,
    BIT4 = 4'd4,
    BIT5 = 4'd5,
    BIT6 = 4'd6,
    BIT7 = 4'd7,
    IDLE = 4'd8,
    STOP = 4'd9,
    OK = 4'd10,
    ERR = 4'd11
  } TopModule_state_t;
  
  TopModule_state_t state_r, state_next;
  
  logic [8-1:0] data_reg;
  
  always_ff @(posedge clk) begin
    if (reset) begin
      state_r <= IDLE;
      data_reg <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        BIT0: begin
          data_reg <= {in, data_reg[7:1]};
        end
        BIT1: begin
          data_reg <= {in, data_reg[7:1]};
        end
        BIT2: begin
          data_reg <= {in, data_reg[7:1]};
        end
        BIT3: begin
          data_reg <= {in, data_reg[7:1]};
        end
        BIT4: begin
          data_reg <= {in, data_reg[7:1]};
        end
        BIT5: begin
          data_reg <= {in, data_reg[7:1]};
        end
        BIT6: begin
          data_reg <= {in, data_reg[7:1]};
        end
        BIT7: begin
          data_reg <= {in, data_reg[7:1]};
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      IDLE: begin
        if (~in) state_next = BIT0;
      end
      BIT0: begin
        state_next = BIT1;
      end
      BIT1: begin
        state_next = BIT2;
      end
      BIT2: begin
        state_next = BIT3;
      end
      BIT3: begin
        state_next = BIT4;
      end
      BIT4: begin
        state_next = BIT5;
      end
      BIT5: begin
        state_next = BIT6;
      end
      BIT6: begin
        state_next = BIT7;
      end
      BIT7: begin
        state_next = STOP;
      end
      STOP: begin
        if (in) state_next = OK;
        else if (~in) state_next = ERR;
      end
      OK: begin
        if (in) state_next = IDLE;
        else if (~in) state_next = BIT0;
      end
      ERR: begin
        if (in) state_next = IDLE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    out_byte = 0;
    done = 1'b0;
    case (state_r)
      IDLE: begin
      end
      BIT0: begin
      end
      BIT1: begin
      end
      BIT2: begin
      end
      BIT3: begin
      end
      BIT4: begin
      end
      BIT5: begin
      end
      BIT6: begin
      end
      BIT7: begin
      end
      STOP: begin
      end
      OK: begin
        done = 1'b1;
        out_byte = data_reg;
      end
      ERR: begin
      end
      default: ;
    endcase
  end

endmodule

