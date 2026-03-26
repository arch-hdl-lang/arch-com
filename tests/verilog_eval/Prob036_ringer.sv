Wrote tests/verilog_eval/Prob036_ringer.sv
put logic vibrate_mode,
  output logic ringer,
  output logic motor
);

  assign ringer = ring & ~vibrate_mode;
  assign motor = ring & vibrate_mode;

endmodule

