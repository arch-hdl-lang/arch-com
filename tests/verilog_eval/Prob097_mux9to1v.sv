module TopModule (
  input logic [16-1:0] a,
  input logic [16-1:0] b,
  input logic [16-1:0] c,
  input logic [16-1:0] d,
  input logic [16-1:0] e,
  input logic [16-1:0] f,
  input logic [16-1:0] g,
  input logic [16-1:0] h,
  input logic [16-1:0] i,
  input logic [4-1:0] sel,
  output logic [16-1:0] out_sig
);

  always_comb begin
    if ((sel == 0)) begin
      out_sig = a;
    end else if ((sel == 1)) begin
      out_sig = b;
    end else if ((sel == 2)) begin
      out_sig = c;
    end else if ((sel == 3)) begin
      out_sig = d;
    end else if ((sel == 4)) begin
      out_sig = e;
    end else if ((sel == 5)) begin
      out_sig = f;
    end else if ((sel == 6)) begin
      out_sig = g;
    end else if ((sel == 7)) begin
      out_sig = h;
    end else if ((sel == 8)) begin
      out_sig = i;
    end else begin
      out_sig = 'hFFFF;
    end
  end

endmodule

