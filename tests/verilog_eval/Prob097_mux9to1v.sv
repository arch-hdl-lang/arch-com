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
  output logic [16-1:0] out
);

  always_comb begin
    if (sel == 0) begin
      out = a;
    end else if (sel == 1) begin
      out = b;
    end else if (sel == 2) begin
      out = c;
    end else if (sel == 3) begin
      out = d;
    end else if (sel == 4) begin
      out = e;
    end else if (sel == 5) begin
      out = f;
    end else if (sel == 6) begin
      out = g;
    end else if (sel == 7) begin
      out = h;
    end else if (sel == 8) begin
      out = i;
    end else begin
      out = 'hFFFF;
    end
  end

endmodule

