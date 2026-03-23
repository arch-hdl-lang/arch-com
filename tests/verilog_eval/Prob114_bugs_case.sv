module TopModule (
  input logic [8-1:0] code,
  output logic [4-1:0] out_sig,
  output logic valid
);

  always_comb begin
    valid = 1;
    if ((code == 'h45)) begin
      out_sig = 0;
    end else if ((code == 'h16)) begin
      out_sig = 1;
    end else if ((code == 'h1E)) begin
      out_sig = 2;
    end else if ((code == 'h26)) begin
      out_sig = 3;
    end else if ((code == 'h25)) begin
      out_sig = 4;
    end else if ((code == 'h2E)) begin
      out_sig = 5;
    end else if ((code == 'h36)) begin
      out_sig = 6;
    end else if ((code == 'h3D)) begin
      out_sig = 7;
    end else if ((code == 'h3E)) begin
      out_sig = 8;
    end else if ((code == 'h46)) begin
      out_sig = 9;
    end else begin
      out_sig = 0;
      valid = 0;
    end
  end

endmodule

