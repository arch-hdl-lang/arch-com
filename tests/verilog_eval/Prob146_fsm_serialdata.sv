// VerilogEval Prob146: Serial receiver with data output
// Start bit (0), 8 data bits LSB-first, stop bit (1). Assert done when stop=1.
// domain SysDomain

module TopModule (
  input logic clk,
  input logic rst,
  input logic in_sig,
  output logic [8-1:0] out_byte,
  output logic done
);

  // States: 8=IDLE, 0-7=BIT0-BIT7, 9=STOP, 10=DONE, 11=ERROR
  logic [4-1:0] state_reg;
  logic [8-1:0] data_reg;
  always_ff @(posedge clk) begin
    if (rst) begin
      data_reg <= 0;
      state_reg <= 8;
    end else begin
      if ((state_reg == 8)) begin
        if ((~in_sig)) begin
          state_reg <= 0;
        end
      end else if ((state_reg == 0)) begin
        for (int i = 0; i <= 6; i++) begin
          data_reg[i] <= data_reg[(i + 1)];
        end
        data_reg[7] <= in_sig;
        state_reg <= 1;
      end else if ((state_reg == 1)) begin
        for (int i = 0; i <= 6; i++) begin
          data_reg[i] <= data_reg[(i + 1)];
        end
        data_reg[7] <= in_sig;
        state_reg <= 2;
      end else if ((state_reg == 2)) begin
        for (int i = 0; i <= 6; i++) begin
          data_reg[i] <= data_reg[(i + 1)];
        end
        data_reg[7] <= in_sig;
        state_reg <= 3;
      end else if ((state_reg == 3)) begin
        for (int i = 0; i <= 6; i++) begin
          data_reg[i] <= data_reg[(i + 1)];
        end
        data_reg[7] <= in_sig;
        state_reg <= 4;
      end else if ((state_reg == 4)) begin
        for (int i = 0; i <= 6; i++) begin
          data_reg[i] <= data_reg[(i + 1)];
        end
        data_reg[7] <= in_sig;
        state_reg <= 5;
      end else if ((state_reg == 5)) begin
        for (int i = 0; i <= 6; i++) begin
          data_reg[i] <= data_reg[(i + 1)];
        end
        data_reg[7] <= in_sig;
        state_reg <= 6;
      end else if ((state_reg == 6)) begin
        for (int i = 0; i <= 6; i++) begin
          data_reg[i] <= data_reg[(i + 1)];
        end
        data_reg[7] <= in_sig;
        state_reg <= 7;
      end else if ((state_reg == 7)) begin
        for (int i = 0; i <= 6; i++) begin
          data_reg[i] <= data_reg[(i + 1)];
        end
        data_reg[7] <= in_sig;
        state_reg <= 9;
      end else if ((state_reg == 9)) begin
        if (in_sig) begin
          state_reg <= 10;
        end else begin
          state_reg <= 11;
        end
      end else if ((state_reg == 10)) begin
        if (in_sig) begin
          state_reg <= 8;
        end else begin
          state_reg <= 0;
        end
      end else if ((state_reg == 11)) begin
        if (in_sig) begin
          state_reg <= 8;
        end
      end
    end
  end
  // IDLE: wait for start bit (in=0)
  // Shift right: data_reg = {in_sig, data_reg[7:1]}
  // STOP: check stop bit
  // DONE: valid stop received
  // ERROR: wait for in=1 then go idle
  logic done_w;
  always_comb begin
    done_w = (state_reg == 10);
    done = done_w;
    if (done_w) begin
      out_byte = data_reg;
    end else begin
      out_byte = 0;
    end
  end

endmodule

// Gate out_byte: valid only when done, else 0
