module TopModule (
  input logic cpu_overheated,
  output logic shut_off_computer,
  input logic arrived,
  input logic gas_tank_empty,
  output logic keep_driving
);

  assign shut_off_computer = cpu_overheated;
  assign keep_driving = ~arrived & ~gas_tank_empty;

endmodule

