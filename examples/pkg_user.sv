package TestPkg;
  localparam int WIDTH = 8;
  typedef enum logic [1:0] {
    ADD = 2'd0,
    SUB = 2'd1,
    AND = 2'd2,
    OR = 2'd3
  } AluOp;
  
  typedef struct packed { // fields: LSB→MSB (reverse of declaration order)
    logic wr;
    logic [32-1:0] data;
    logic [32-1:0] addr;
  } BusReq;
  
  function automatic logic [8-1:0] max(input logic [8-1:0] a, input logic [8-1:0] b);
    return a > b ? a : b;
  endfunction
  
endpackage

import TestPkg::*;
module PkgUser (
  input AluOp op,
  input logic [8-1:0] a,
  input logic [8-1:0] b,
  output logic [8-1:0] result
);

  function automatic logic [8-1:0] max(input logic [8-1:0] a, input logic [8-1:0] b);
    return a > b ? a : b;
  endfunction
  
  assign result = max(a, b);

endmodule

