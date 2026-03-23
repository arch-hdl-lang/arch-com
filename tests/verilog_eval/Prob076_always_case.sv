module TopModule (
  input logic [3-1:0] sel,
  input logic [4-1:0] data0,
  input logic [4-1:0] data1,
  input logic [4-1:0] data2,
  input logic [4-1:0] data3,
  input logic [4-1:0] data4,
  input logic [4-1:0] data5,
  output logic [4-1:0] out_sig
);

  always_comb begin
    if ((sel == 0)) begin
      out_sig = data0;
    end else if ((sel == 1)) begin
      out_sig = data1;
    end else if ((sel == 2)) begin
      out_sig = data2;
    end else if ((sel == 3)) begin
      out_sig = data3;
    end else if ((sel == 4)) begin
      out_sig = data4;
    end else if ((sel == 5)) begin
      out_sig = data5;
    end else begin
      out_sig = 0;
    end
  end

endmodule

