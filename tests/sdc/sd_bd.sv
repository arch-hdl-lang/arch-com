// SD Buffer Descriptor Manager
// RAM_MEM_WIDTH_16 variant: 16-bit x 32 RAM, 4 writes per BD (2x16-bit address + 2x16-bit data).
// free_bd resets to BD_SIZE/4 = 8. Matches OpenCores SDC reference.
module sd_bd (
  input logic clk,
  input logic rst,
  input logic we_m,
  input logic [16-1:0] dat_in_m,
  output logic [5-1:0] free_bd,
  input logic re_s,
  output logic ack_o_s,
  input logic a_cmp,
  output logic [16-1:0] dat_out_s
);

  // BD memory: 32 entries x 16 bits
  logic [16-1:0] bd_mem [32-1:0];
  // Master write pointer (5-bit, indexes into bd_mem)
  logic [5-1:0] m_wr_pnt;
  // Write counter: tracks 4 writes per BD (0,1,2,3)
  logic [2-1:0] write_cnt;
  // new_bw: pulses when a BD is fully written
  logic new_bw;
  // Free BD counter: starts at BD_SIZE/4 = 8
  logic [5-1:0] free_bd_r;
  // last_a_cmp: edge detection for a_cmp
  logic last_a_cmp;
  // Slave read pointer
  logic [5-1:0] s_rd_pnt;
  // Slave read sub-word counter
  logic [2-1:0] read_s_cnt;
  // Slave ack
  logic ack_o_s_r;
  // Slave data output
  logic [16-1:0] dat_out_s_r;
  // Master write logic
  always_ff @(posedge clk or posedge rst) begin
    if (rst) begin
      m_wr_pnt <= 0;
      new_bw <= 1'b0;
      write_cnt <= 0;
    end else begin
      new_bw <= 1'b0;
      if (we_m) begin
        if (free_bd_r > 0) begin
          write_cnt <= 2'(write_cnt + 1);
          m_wr_pnt <= 5'(m_wr_pnt + 1);
          if (~write_cnt[1]) begin
            // First two writes: address part
            bd_mem[m_wr_pnt] <= dat_in_m;
          end else begin
            // Second two writes: data part
            bd_mem[m_wr_pnt] <= dat_in_m;
            new_bw <= write_cnt[0];
          end
        end
      end
      // Complete BD on 4th write (cnt goes 0->1->2->3)
    end
  end
  // Free BD counter
  always_ff @(posedge clk or posedge rst) begin
    if (rst) begin
      free_bd_r <= 8;
      last_a_cmp <= 1'b0;
    end else begin
      if (new_bw) begin
        free_bd_r <= 5'(free_bd_r - 1);
      end else if (a_cmp) begin
        last_a_cmp <= a_cmp;
        if (~last_a_cmp) begin
          free_bd_r <= 5'(free_bd_r + 1);
        end
      end else begin
        last_a_cmp <= a_cmp;
      end
    end
  end
  // Slave read logic
  always_ff @(posedge clk or posedge rst) begin
    if (rst) begin
      ack_o_s_r <= 1'b0;
      dat_out_s_r <= 0;
      read_s_cnt <= 0;
      s_rd_pnt <= 0;
    end else begin
      ack_o_s_r <= 1'b0;
      if (re_s) begin
        read_s_cnt <= 2'(read_s_cnt + 1);
        s_rd_pnt <= 5'(s_rd_pnt + 1);
        ack_o_s_r <= 1'b1;
        if (~read_s_cnt[1]) begin
          dat_out_s_r <= bd_mem[s_rd_pnt];
        end else begin
          dat_out_s_r <= bd_mem[s_rd_pnt];
        end
      end
    end
  end
  assign free_bd = free_bd_r;
  assign ack_o_s = ack_o_s_r;
  assign dat_out_s = dat_out_s_r;

endmodule

