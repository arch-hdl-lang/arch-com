module TopModule (
  input logic cpu_overheated,
  output logic shut_off_computer,
  input logic arrived,
  input logic gas_tank_empty,
  output logic keep_driving
);

  always_comb begin
    if (cpu_overheated) begin
      shut_off_computer = 1;
    end else begin
      shut_off_computer = 0;
    end
    if ((~arrived)) begin
      keep_driving = (~gas_tank_empty);
    end else begin
      keep_driving = 0;
    end
  end

endmodule

