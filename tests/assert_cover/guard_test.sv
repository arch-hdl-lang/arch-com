module GuardTest (
  input logic clk,
  input logic rst,
  input logic en,
  input logic [31:0] din,
  output logic [31:0] dout,
  output logic dout_valid
);

  logic [31:0] data;
  logic valid_r;
  always_ff @(posedge clk) begin
    if (rst) begin
      valid_r <= 1'b0;
    end else begin
      if (en) begin
        data <= din;
        valid_r <= 1'b1;
      end
    end
  end
  assign dout = data;
  assign dout_valid = valid_r;
  
  // synopsys translate_off
  // Guard-contract shadow regs + SVA (one per `reg ... guard <sig>`)
  logic _data_written;
  always_ff @(posedge clk) begin
    if (rst) _data_written <= 1'b0;
    else if (((en))) _data_written <= 1'b1;
  end
  _data_guard_contract: assert property (@(posedge clk) disable iff (rst) valid_r |-> _data_written)
    else $fatal(1, "GUARD VIOLATION: GuardTest.data — valid_r asserted but data never written");
  // synopsys translate_on

endmodule

