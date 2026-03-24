module TopModule (
  input logic [8-1:0] code,
  output logic [4-1:0] out,
  output logic valid
);

  always_comb begin
    valid = 1;
    if ((code == 'h45)) begin
      out = 0;
    end else if ((code == 'h16)) begin
      out = 1;
    end else if ((code == 'h1E)) begin
      out = 2;
    end else if ((code == 'h26)) begin
      out = 3;
    end else if ((code == 'h25)) begin
      out = 4;
    end else if ((code == 'h2E)) begin
      out = 5;
    end else if ((code == 'h36)) begin
      out = 6;
    end else if ((code == 'h3D)) begin
      out = 7;
    end else if ((code == 'h3E)) begin
      out = 8;
    end else if ((code == 'h46)) begin
      out = 9;
    end else begin
      out = 0;
      valid = 0;
    end
  end

endmodule

