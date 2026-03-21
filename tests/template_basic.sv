// domain SysDomain
//   freq_mhz: 100

module MyArbiter #(
  parameter int NUM_REQ = 4
) (
  input logic clk,
  input logic rst,
  input logic [4-1:0] req_mask,
  output logic grant_valid,
  output logic [4-1:0] grant_out
);

  function automatic logic [4-1:0] FixedGrant(input logic [4-1:0] req_mask);
    return (req_mask & 4'(((~req_mask) + 1)));
  endfunction
  
  logic [4-1:0] grant;
  assign grant = FixedGrant(req_mask);
  assign grant_valid = (grant != 0);
  assign grant_out = grant;

endmodule

