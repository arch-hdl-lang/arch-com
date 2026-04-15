// Round-robin grant function for 2-requester bus arbiter.
// When both request, prefer the one not last granted.
// Returns one-hot grant mask.
// deprioritize last winner; if masked set is empty, fall back to all requesters
// isolate lowest set bit: pick & (-pick) via pick & ((pick^3)+1)
// 2-requester round-robin bus arbiter using the arbiter construct
// with a custom policy function (BusArbRRGrant).
module bus_arbiter_rr #(
  parameter int NUM_REQ = 2
) (
  input logic clk,
  input logic rst,
  output logic grant_valid,
  output logic [0:0] grant_requester,
  input logic [NUM_REQ-1:0] request_valid,
  output logic [NUM_REQ-1:0] request_ready
);

  function automatic logic [1:0] BusArbRRGrant(input logic [1:0] req_mask, input logic [1:0] last_grant);
    logic both = req_mask == 3;
    logic [1:0] masked = req_mask & (last_grant ^ 3);
    logic [1:0] pick = both ? masked : req_mask;
    logic [2:0] pick_neg = 3'($unsigned(pick ^ 3)) + 1;
    return pick & 2'(pick_neg);
  endfunction
  
  logic [1:0] last_grant_r;
  
  always_ff @(posedge clk or posedge rst) begin
    if (rst) last_grant_r <= '0;
    else if (grant_valid) last_grant_r <= grant_onehot;
  end
  
  logic [1:0] grant_onehot;
  
  always_comb begin
    grant_onehot = BusArbRRGrant(request_valid, last_grant_r);
    grant_valid = |grant_onehot;
    request_ready = grant_onehot;
    grant_requester = '0;
    for (int ci = 0; ci < 2; ci++) begin
      if (grant_onehot[ci]) grant_requester = 1'(ci);
    end
  end

endmodule

