// VerilogEval Prob154: PS/2 3-byte message FSM with data output
// Find byte with in[3]=1, collect 3 bytes, assert done
// domain SysDomain

module TopModule (
  input logic clk,
  input logic reset,
  input logic [8-1:0] in,
  output logic [24-1:0] out_bytes,
  output logic done
);

  logic [2-1:0] state_reg;
  // 24-bit shift register: always shift in new byte
  logic [24-1:0] out_r;
  // States: 0=FIND, 1=GOT1, 2=GOT2, 3=DONE
  // Always shift in: out_r <= {out_r[15:0], in}
  always_ff @(posedge clk) begin
    if (reset) begin
      out_r <= 0;
      state_reg <= 0;
    end else begin
      for (int i = 0; i <= 15; i++) begin
        out_r[(i + 8)] <= out_r[i];
      end
      for (int i = 0; i <= 7; i++) begin
        out_r[i] <= in[i];
      end
      if ((state_reg == 0)) begin
        if (in[3]) begin
          state_reg <= 1;
        end
      end else if ((state_reg == 1)) begin
        state_reg <= 2;
      end else if ((state_reg == 2)) begin
        state_reg <= 3;
      end else if (in[3]) begin
        state_reg <= 1;
      end else begin
        state_reg <= 0;
      end
    end
  end
  always_comb begin
    done = (state_reg == 3);
    if ((state_reg == 3)) begin
      for (int i = 0; i <= 23; i++) begin
        out_bytes[i] = out_r[i];
      end
    end else begin
      for (int i = 0; i <= 23; i++) begin
        out_bytes[i] = 1'b0;
      end
    end
  end

endmodule

// Output shift register when done, else 0
