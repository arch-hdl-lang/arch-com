// VerilogEval Prob156: Full timer FSM - detect 1101, shift 4-bit delay, count down, done+ack
// domain SysDomain

module TopModule (
  input logic clk,
  input logic reset,
  input logic data,
  input logic ack,
  output logic [4-1:0] count,
  output logic counting,
  output logic done
);

  // FSM states: 0=S, 1=S1, 2=S11, 3=S110
  // 4=B0, 5=B1, 6=B2, 7=B3
  // 8=COUNT, 9=DONE_ST
  logic [4-1:0] state_reg;
  logic [4-1:0] delay_reg;
  logic [4-1:0] cnt_reg;
  logic [10-1:0] sub_cnt;
  always_ff @(posedge clk) begin
    if (reset) begin
      cnt_reg <= 0;
      delay_reg <= 0;
      state_reg <= 0;
      sub_cnt <= 0;
    end else begin
      if ((state_reg == 0)) begin
        if (data) begin
          state_reg <= 1;
        end
      end else if ((state_reg == 1)) begin
        if (data) begin
          state_reg <= 2;
        end else begin
          state_reg <= 0;
        end
      end else if ((state_reg == 2)) begin
        if ((~data)) begin
          state_reg <= 3;
        end
      end else if ((state_reg == 3)) begin
        if (data) begin
          state_reg <= 4;
        end else begin
          state_reg <= 0;
        end
      end else if ((state_reg == 4)) begin
        delay_reg[3] <= data;
        state_reg <= 5;
      end else if ((state_reg == 5)) begin
        delay_reg[2] <= data;
        state_reg <= 6;
      end else if ((state_reg == 6)) begin
        delay_reg[1] <= data;
        state_reg <= 7;
      end else if ((state_reg == 7)) begin
        delay_reg[0] <= data;
        cnt_reg[0] <= data;
        cnt_reg[1] <= delay_reg[1];
        cnt_reg[2] <= delay_reg[2];
        cnt_reg[3] <= delay_reg[3];
        sub_cnt <= 0;
        state_reg <= 8;
      end else if ((state_reg == 8)) begin
        if (((sub_cnt == 999) & (cnt_reg == 0))) begin
          state_reg <= 9;
        end else if ((sub_cnt == 999)) begin
          sub_cnt <= 0;
          cnt_reg <= 4'((cnt_reg - 1));
        end else begin
          sub_cnt <= 10'((sub_cnt + 1));
        end
      end else if ((state_reg == 9)) begin
        if (ack) begin
          state_reg <= 0;
        end
      end
    end
  end
  // Last bit shifted in; go directly to COUNT
  // Init counter: cnt starts at delay value, sub_cnt at 0
  // But delay[0] isn't stored yet, so compute initial cnt from {delay_reg[3:1], data}
  // COUNT
  assign counting = (state_reg == 8);
  assign done = (state_reg == 9);
  assign count = cnt_reg;

endmodule

