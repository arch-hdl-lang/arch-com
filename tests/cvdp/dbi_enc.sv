module dbi_enc (
  input logic clk,
  input logic rst_n,
  input logic [40-1:0] data_in,
  output logic [40-1:0] data_out,
  output logic [2-1:0] dbi_cntrl
);

  logic [40-1:0] prev_data;
  logic [20-1:0] group1;
  assign group1 = data_in[39:20];
  logic [20-1:0] group0;
  assign group0 = data_in[19:0];
  logic [20-1:0] prev1;
  assign prev1 = prev_data[39:20];
  logic [20-1:0] prev0;
  assign prev0 = prev_data[19:0];
  logic [20-1:0] xor1;
  assign xor1 = group1 ^ prev1;
  logic [20-1:0] xor0;
  assign xor0 = group0 ^ prev0;
  logic [5-1:0] cnt1;
  logic [5-1:0] cnt0;
  assign cnt1 = 5'(5'($unsigned(xor1[0])) + 5'($unsigned(xor1[1])) + 5'($unsigned(xor1[2])) + 5'($unsigned(xor1[3])) + 5'($unsigned(xor1[4])) + 5'($unsigned(xor1[5])) + 5'($unsigned(xor1[6])) + 5'($unsigned(xor1[7])) + 5'($unsigned(xor1[8])) + 5'($unsigned(xor1[9])) + 5'($unsigned(xor1[10])) + 5'($unsigned(xor1[11])) + 5'($unsigned(xor1[12])) + 5'($unsigned(xor1[13])) + 5'($unsigned(xor1[14])) + 5'($unsigned(xor1[15])) + 5'($unsigned(xor1[16])) + 5'($unsigned(xor1[17])) + 5'($unsigned(xor1[18])) + 5'($unsigned(xor1[19])));
  assign cnt0 = 5'(5'($unsigned(xor0[0])) + 5'($unsigned(xor0[1])) + 5'($unsigned(xor0[2])) + 5'($unsigned(xor0[3])) + 5'($unsigned(xor0[4])) + 5'($unsigned(xor0[5])) + 5'($unsigned(xor0[6])) + 5'($unsigned(xor0[7])) + 5'($unsigned(xor0[8])) + 5'($unsigned(xor0[9])) + 5'($unsigned(xor0[10])) + 5'($unsigned(xor0[11])) + 5'($unsigned(xor0[12])) + 5'($unsigned(xor0[13])) + 5'($unsigned(xor0[14])) + 5'($unsigned(xor0[15])) + 5'($unsigned(xor0[16])) + 5'($unsigned(xor0[17])) + 5'($unsigned(xor0[18])) + 5'($unsigned(xor0[19])));
  logic ctrl1;
  logic ctrl0;
  logic [20-1:0] out_hi;
  logic [20-1:0] out_lo;
  always_comb begin
    ctrl1 = cnt1 > 10;
    ctrl0 = cnt0 > 10;
    if (ctrl1) begin
      out_hi = ~group1;
    end else begin
      out_hi = group1;
    end
    if (ctrl0) begin
      out_lo = ~group0;
    end else begin
      out_lo = group0;
    end
  end
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      data_out <= 0;
      dbi_cntrl <= 0;
      prev_data <= 0;
    end else begin
      dbi_cntrl <= {ctrl1, ctrl0};
      data_out <= {out_hi, out_lo};
      prev_data <= {out_hi, out_lo};
    end
  end

endmodule

