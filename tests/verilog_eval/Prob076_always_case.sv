module TopModule (
  input logic [3-1:0] sel,
  input logic [4-1:0] data0,
  input logic [4-1:0] data1,
  input logic [4-1:0] data2,
  input logic [4-1:0] data3,
  input logic [4-1:0] data4,
  input logic [4-1:0] data5,
  output logic [4-1:0] out
);

  always_comb begin
    if ((sel == 0)) begin
      out = data0;
    end else if ((sel == 1)) begin
      out = data1;
    end else if ((sel == 2)) begin
      out = data2;
    end else if ((sel == 3)) begin
      out = data3;
    end else if ((sel == 4)) begin
      out = data4;
    end else if ((sel == 5)) begin
      out = data5;
    end else begin
      out = 0;
    end
  end

endmodule

