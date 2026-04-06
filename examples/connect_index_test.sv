// Integration test for:
//   1. generate_for + connect port.member <- signal[i]   (dotted port + Vec element as source)
//   2. connect port.member -> signal[i]                  (dotted port + Vec element as sink)
//
// Pattern: 2-way tag store — each way has a RAM-like port bundle.
module TinyRam (
  input logic clk,
  input logic wr_en,
  input logic [4-1:0] wr_addr,
  input logic [8-1:0] wr_data,
  input logic [4-1:0] rd_addr,
  output logic [8-1:0] rd_data
);

  logic [8-1:0] mem [16-1:0];
  always_ff @(posedge clk) begin
    for (int j = 0; j <= 15; j++) begin
      if (wr_en & wr_addr == 4'(j)) begin
        mem[j] <= wr_data;
      end
    end
  end
  always_comb begin
    rd_data = 0;
    for (int j = 0; j <= 15; j++) begin
      if (rd_addr == 4'(j)) begin
        rd_data = mem[j];
      end
    end
  end

endmodule

// Wrapper: 2 ways, Vec-typed wires, connected via generate_for + dotted port names.
module ConnectIndexTest (
  input logic clk,
  input logic rst,
  input logic wr_en,
  input logic [1-1:0] wr_way,
  input logic [4-1:0] wr_addr,
  input logic [8-1:0] wr_data,
  input logic [4-1:0] rd_addr,
  output logic [8-1:0] rd_data0,
  output logic [8-1:0] rd_data1
);

  logic wr_en_w [2-1:0];
  logic [8-1:0] rd_data_w [2-1:0];
  assign wr_en_w[0] = wr_en & wr_way == 'b0;
  assign wr_en_w[1] = wr_en & wr_way == 'b1;
  // Only the selected way gets the write
  TinyRam ram_0 (
    .clk(clk),
    .wr_en(wr_en_w[0]),
    .wr_addr(wr_addr),
    .wr_data(wr_data),
    .rd_addr(rd_addr),
    .rd_data(rd_data_w[0])
  );
  TinyRam ram_1 (
    .clk(clk),
    .wr_en(wr_en_w[1]),
    .wr_addr(wr_addr),
    .wr_data(wr_data),
    .rd_addr(rd_addr),
    .rd_data(rd_data_w[1])
  );
  assign rd_data0 = rd_data_w[0];
  assign rd_data1 = rd_data_w[1];

endmodule

