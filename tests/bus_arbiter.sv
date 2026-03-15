// domain SysDomain
//   freq_mhz: 100

module BusArbiter #(
  parameter int NUM_REQ = 4
) (
  input logic clk,
  input logic rst,
  output logic grant_valid,
  output logic [2-1:0] grant_requester,
  input logic [NUM_REQ-1:0] request_valid,
  output logic [NUM_REQ-1:0] request_ready
);

  logic [2-1:0] rr_ptr_r;
  integer arb_i;
  logic arb_found;
  
  always_ff @(posedge clk) begin
    if (rst) rr_ptr_r <= '0;
    else if (grant_valid) rr_ptr_r <= rr_ptr_r + 1;
  end
  
  always_comb begin
    grant_valid = 1'b0;
    request_ready = '0;
    grant_requester = '0;
    arb_found = 1'b0;
    for (arb_i = 0; arb_i < 4; arb_i++) begin
      if (!arb_found && request_valid[(int'(rr_ptr_r) + arb_i) % 4]) begin
        arb_found = 1'b1;
        grant_valid = 1'b1;
        grant_requester = 2'((int'(rr_ptr_r) + arb_i) % 4);
        request_ready[(int'(rr_ptr_r) + arb_i) % 4] = 1'b1;
      end
    end
  end

endmodule

