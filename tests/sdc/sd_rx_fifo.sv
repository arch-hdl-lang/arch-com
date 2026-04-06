// SD RX FIFO (Dual-clock, 4-bit input -> 32-bit output, depth 8)
// Nibble packing: accumulates 8 nibbles into 32-bit word.
// BIG_ENDIAN: we[7] stores first nibble at tmp[3:0] (LSB), we[0] stores last at tmp[31:28].
// RAM write triggers on we[0] & ft. Matches OpenCores SDC reference (BIG_ENDIAN config).
module sd_rx_fifo (
  input logic wclk,
  input logic rclk,
  input logic rst,
  input logic [4-1:0] d,
  input logic wr,
  output logic [32-1:0] q,
  input logic rd,
  output logic full,
  output logic empty,
  output logic [2-1:0] mem_empt
);

  logic [32-1:0] mem [8-1:0];
  logic [4-1:0] adr_i;
  logic [4-1:0] adr_o;
  logic [8-1:0] we_r;
  logic [32-1:0] tmp;
  logic ft;
  logic full_w;
  assign full_w = adr_i[2:0] == adr_o[2:0] & adr_i[3:3] != adr_o[3:3];
  logic empty_w;
  assign empty_w = adr_i == adr_o;
  // ram_we: write to memory when we[0] is active AND ft is set AND wr is active
  logic ram_we_w;
  always_ff @(posedge wclk or posedge rst) begin
    if (rst) begin
      ft <= 1'b0;
      tmp <= 0;
      we_r <= 1;
    end else begin
      if (wr) begin
        we_r <= {we_r[6:0], we_r[7:7]};
        // BIG_ENDIAN nibble packing (matching reference sd_defines.v)
        if (we_r[7:7]) begin
          tmp[3:0] <= d;
          ft <= 1'b1;
        end
        if (we_r[6:6]) begin
          tmp[7:4] <= d;
        end
        if (we_r[5:5]) begin
          tmp[11:8] <= d;
        end
        if (we_r[4:4]) begin
          tmp[15:12] <= d;
        end
        if (we_r[3:3]) begin
          tmp[19:16] <= d;
        end
        if (we_r[2:2]) begin
          tmp[23:20] <= d;
        end
        if (we_r[1:1]) begin
          tmp[27:24] <= d;
        end
        if (we_r[0:0]) begin
          tmp[31:28] <= d;
        end
      end
    end
  end
  // RAM write: separate so it uses OLD tmp value (before current nibble update)
  // In the reference, ram_we = wr & we[0] & ft is combinational, and
  // ram[adr_i] <= ram_din happens in a separate always block.
  // Since ARCH uses non-blocking, tmp in the same seq block still holds old value.
  assign ram_we_w = wr & we_r[0:0] & ft;
  always_ff @(posedge wclk) begin
    if (ram_we_w) begin
      mem[adr_i[2:0]] <= tmp;
    end
  end
  // Write address counter
  always_ff @(posedge wclk or posedge rst) begin
    if (rst) begin
      adr_i <= 0;
    end else begin
      if (ram_we_w) begin
        if (adr_i == 7) begin
          adr_i[2:0] <= 0;
          adr_i[3:3] <= ~adr_i[3:3];
        end else begin
          adr_i <= 4'(adr_i + 1);
        end
      end
    end
  end
  // Read address counter
  always_ff @(posedge rclk or posedge rst) begin
    if (rst) begin
      adr_o <= 0;
    end else begin
      if (~empty_w & rd) begin
        if (adr_o == 7) begin
          adr_o[2:0] <= 0;
          adr_o[3:3] <= ~adr_o[3:3];
        end else begin
          adr_o <= 4'(adr_o + 1);
        end
      end
    end
  end
  assign q = mem[adr_o[2:0]];
  assign full = full_w;
  assign empty = empty_w;
  assign mem_empt = 2'(adr_i - adr_o);

endmodule

