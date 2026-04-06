// Test: function construct
// Xtime: GF(2^8) multiply by 2
module XtimeTest (
  input logic [8-1:0] a,
  output logic [8-1:0] y
);

  function automatic logic [8-1:0] Xtime(input logic [8-1:0] a);
    logic [8-1:0] shifted = 8'(a << 1);
    return ((a & 'h80 == 'h80) ? shifted ^ 'h1B : shifted);
  endfunction
  
  assign y = Xtime(a);

endmodule

