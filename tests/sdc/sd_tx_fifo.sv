// SD TX FIFO (Dual-clock, 32-bit wide, depth 8)
// Write side on wclk, read side on rclk.
module sd_tx_fifo (
  input logic wclk,
  input logic rclk,
  input logic rst,
  input logic [32-1:0] d,
  input logic wr,
  output logic [32-1:0] q,
  input logic rd,
  output logic full,
  output logic empty,
  output logic [6-1:0] mem_empt
);

  logic [32-1:0] mem [8-1:0];
  logic [4-1:0] adr_i;
  logic [4-1:0] adr_o;
  logic full_w;
  assign full_w = adr_i[2:0] == adr_o[2:0] & adr_i[3:3] != adr_o[3:3];
  logic empty_w;
  assign empty_w = adr_i == adr_o;
  always_ff @(posedge wclk or posedge rst) begin
    if (rst) begin
      adr_i <= 0;
    end else begin
      if (wr & ~full_w) begin
        mem[adr_i[2:0]] <= d;
        adr_i <= 4'(adr_i + 1);
      end
    end
  end
  always_ff @(posedge rclk or posedge rst) begin
    if (rst) begin
      adr_o <= 0;
    end else begin
      if (rd & ~empty_w) begin
        adr_o <= 4'(adr_o + 1);
      end
    end
  end
  assign q = mem[adr_o[2:0]];
  assign full = full_w;
  assign empty = empty_w;
  assign mem_empt = 6'($unsigned(adr_i - adr_o));

endmodule

