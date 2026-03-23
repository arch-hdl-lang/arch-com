module TopModule (
  input logic ring,
  input logic vibrate_mode,
  output logic ringer,
  output logic motor
);

  assign ringer = (ring & (~vibrate_mode));
  assign motor = (ring & vibrate_mode);

endmodule

