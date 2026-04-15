module sdram_controller (
  input logic clk,
  input logic reset,
  input logic [23:0] addr,
  input logic [15:0] data_in,
  output logic [15:0] data_out,
  input logic read,
  input logic write,
  output logic sdram_clk,
  output logic sdram_cke,
  output logic sdram_cs,
  output logic sdram_ras,
  output logic sdram_cas,
  output logic sdram_we,
  output logic [12:0] sdram_addr,
  output logic [1:0] sdram_ba,
  input logic [15:0] sdram_dq,
  output logic [15:0] dq_out
);

  logic [2:0] state;
  logic [3:0] init_cnt;
  logic [10:0] refresh_cnt;
  logic [15:0] data_out_r;
  logic [15:0] dq_out_r;
  logic sdram_cs_r;
  logic sdram_ras_r;
  logic sdram_cas_r;
  logic sdram_we_r;
  logic [12:0] sdram_addr_r;
  logic [1:0] sdram_ba_r;
  logic read_pending;
  assign sdram_clk = clk;
  assign sdram_cke = 1'b1;
  assign sdram_cs = sdram_cs_r;
  assign sdram_ras = sdram_ras_r;
  assign sdram_cas = sdram_cas_r;
  assign sdram_we = sdram_we_r;
  assign sdram_addr = sdram_addr_r;
  assign sdram_ba = sdram_ba_r;
  assign data_out = data_out_r;
  assign dq_out = dq_out_r;
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      data_out_r <= 0;
      dq_out_r <= 0;
      init_cnt <= 0;
      read_pending <= 1'b0;
      refresh_cnt <= 0;
      sdram_addr_r <= 0;
      sdram_ba_r <= 0;
      sdram_cas_r <= 1'b0;
      sdram_cs_r <= 1'b0;
      sdram_ras_r <= 1'b0;
      sdram_we_r <= 1'b0;
      state <= 0;
    end else begin
      if (state == 0) begin
        // INIT state
        if (init_cnt == 9) begin
          state <= 1;
          init_cnt <= 0;
        end else begin
          init_cnt <= 4'(init_cnt + 1);
        end
        sdram_cs_r <= 1'b0;
        sdram_ras_r <= 1'b0;
        sdram_cas_r <= 1'b0;
        sdram_we_r <= 1'b0;
      end else if (state == 1) begin
        // IDLE state
        if (read | write) begin
          state <= 2;
          sdram_cs_r <= 1'b1;
          sdram_ras_r <= 1'b1;
          sdram_cas_r <= 1'b1;
          sdram_we_r <= 1'b0;
          sdram_addr_r <= addr[23:11];
          sdram_ba_r <= addr[10:9];
          read_pending <= read;
          refresh_cnt <= 0;
        end else if (refresh_cnt == 1024) begin
          state <= 5;
          sdram_cs_r <= 1'b1;
          sdram_ras_r <= 1'b1;
          sdram_cas_r <= 1'b1;
          sdram_we_r <= 1'b0;
          refresh_cnt <= 0;
        end else begin
          refresh_cnt <= 11'(refresh_cnt + 1);
          sdram_cs_r <= 1'b0;
          sdram_ras_r <= 1'b0;
          sdram_cas_r <= 1'b0;
          sdram_we_r <= 1'b0;
        end
      end else if (state == 2) begin
        if (read_pending) begin
          state <= 3;
          sdram_cs_r <= 1'b1;
          sdram_ras_r <= 1'b0;
          sdram_cas_r <= 1'b1;
          sdram_we_r <= 1'b0;
          sdram_addr_r <= 13'($unsigned(addr[8:0]));
        end else begin
          state <= 4;
          sdram_cs_r <= 1'b1;
          sdram_ras_r <= 1'b0;
          sdram_cas_r <= 1'b1;
          sdram_we_r <= 1'b1;
          sdram_addr_r <= 13'($unsigned(addr[8:0]));
        end
      end else if (state == 3) begin
        data_out_r <= sdram_dq;
        state <= 1;
        sdram_cs_r <= 1'b0;
        sdram_ras_r <= 1'b0;
        sdram_cas_r <= 1'b0;
        sdram_we_r <= 1'b0;
      end else if (state == 4) begin
        dq_out_r <= data_in;
        state <= 1;
        sdram_cs_r <= 1'b0;
        sdram_ras_r <= 1'b0;
        sdram_cas_r <= 1'b0;
        sdram_we_r <= 1'b0;
      end else if (state == 5) begin
        state <= 1;
        sdram_cs_r <= 1'b0;
        sdram_ras_r <= 1'b0;
        sdram_cas_r <= 1'b0;
        sdram_we_r <= 1'b0;
      end
    end
  end

endmodule

