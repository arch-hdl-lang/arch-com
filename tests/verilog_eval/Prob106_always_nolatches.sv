Wrote tests/verilog_eval/Prob106_always_nolatches.sv
 output logic left,
  output logic down,
  output logic right,
  output logic up
);

  always_comb begin
    left = 0;
    down = 0;
    right = 0;
    up = 0;
    if (scancode == 'hE06B) begin
      left = 1;
    end else if (scancode == 'hE072) begin
      down = 1;
    end else if (scancode == 'hE074) begin
      right = 1;
    end else if (scancode == 'hE075) begin
      up = 1;
    end
  end

endmodule

