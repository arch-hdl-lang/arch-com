Wrote tests/verilog_eval/Prob156_review2015_fancytimer.sv
t 4-bit delay, count down, done+ack
module TopModule (
  input logic clk,
  input logic reset,
  input logic data,
  input logic ack,
  output logic [4-1:0] count,
  output logic counting,
  output logic done
);

  typedef enum logic [3:0] {
    S = 4'd0,
    S1 = 4'd1,
    S11 = 4'd2,
    S110 = 4'd3,
    B0 = 4'd4,
    B1 = 4'd5,
    B2 = 4'd6,
    B3 = 4'd7,
    COUNT_ST = 4'd8,
    DONE_ST = 4'd9
  } TopModule_state_t;
  
  TopModule_state_t state_r, state_next;
  
  logic [4-1:0] delay_r;
  logic [4-1:0] cnt_r;
  logic [10-1:0] sub_cnt;
  
  always_ff @(posedge clk) begin
    if (reset) begin
      state_r <= S;
      delay_r <= 0;
      cnt_r <= 0;
      sub_cnt <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        B0: begin
          delay_r[3] <= data;
        end
        B1: begin
          delay_r[2] <= data;
        end
        B2: begin
          delay_r[1] <= data;
        end
        B3: begin
          delay_r[0] <= data;
          cnt_r[3] <= delay_r[3];
          cnt_r[2] <= delay_r[2];
          cnt_r[1] <= delay_r[1];
          cnt_r[0] <= data;
          sub_cnt <= 0;
        end
        COUNT_ST: begin
          if (sub_cnt == 999 & cnt_r == 0) begin
            sub_cnt <= sub_cnt;
            cnt_r <= cnt_r;
          end else if (sub_cnt == 999) begin
            sub_cnt <= 0;
            cnt_r <= 4'(cnt_r - 1);
          end else begin
            sub_cnt <= 10'(sub_cnt + 1);
          end
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      S: begin
        if (data) state_next = S1;
      end
      S1: begin
        if (data) state_next = S11;
        else if (~data) state_next = S;
      end
      S11: begin
        if (~data) state_next = S110;
      end
      S110: begin
        if (data) state_next = B0;
        else if (~data) state_next = S;
      end
      B0: begin
        state_next = B1;
      end
      B1: begin
        state_next = B2;
      end
      B2: begin
        state_next = B3;
      end
      B3: begin
        state_next = COUNT_ST;
      end
      COUNT_ST: begin
        if (sub_cnt == 999 & cnt_r == 0) state_next = DONE_ST;
      end
      DONE_ST: begin
        if (ack) state_next = S;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    count = 0;
    counting = 1'b0;
    done = 1'b0;
    case (state_r)
      S: begin
      end
      S1: begin
      end
      S11: begin
      end
      S110: begin
      end
      B0: begin
      end
      B1: begin
      end
      B2: begin
      end
      B3: begin
      end
      COUNT_ST: begin
        counting = 1'b1;
        count = cnt_r;
      end
      DONE_ST: begin
        done = 1'b1;
        count = cnt_r;
      end
      default: ;
    endcase
  end

endmodule

