module dbi_dec (
  input logic clk,
  input logic rst_n,
  input logic [39:0] data_in,
  input logic [1:0] dbi_cntrl,
  output logic [39:0] data_out
);

  // Group-0 = data_in[19:0], Group-1 = data_in[39:20]
  // If dbi_cntrl[0]=1, invert Group-0; if dbi_cntrl[1]=1, invert Group-1
  logic [39:0] decoded;
  always_comb begin
    decoded[19:0] = data_in[19:0];
    decoded[39:20] = data_in[39:20];
    if (dbi_cntrl[0:0] == 1) begin
      decoded[19:0] = ~data_in[19:0];
    end
    if (dbi_cntrl[1:1] == 1) begin
      decoded[39:20] = ~data_in[39:20];
    end
  end
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      data_out <= 0;
    end else begin
      data_out <= decoded;
    end
  end

endmodule

