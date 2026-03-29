module apb_dsp_unit (
  input logic pclk,
  input logic presetn,
  input logic [10-1:0] paddr,
  input logic pselx,
  input logic penable,
  input logic pwrite,
  input logic [8-1:0] pwdata,
  input logic sram_valid,
  output logic pready,
  output logic [8-1:0] prdata,
  output logic pslverr
);

  // FSM state: 0=IDLE, 1=WRITE_ACCESS, 2=READ_ACCESS
  logic [2-1:0] state;
  // Config registers
  logic [10-1:0] r_operand_1;
  logic [10-1:0] r_operand_2;
  logic [8-1:0] r_Enable;
  logic [10-1:0] r_write_address;
  logic [8-1:0] r_write_data;
  // SRAM 1KB - init only
  logic [8-1:0] mem [0:1024-1];
  // Result register at address 0x5
  logic [8-1:0] r_result;
  // SRAM write on posedge sram_valid
  always_ff @(posedge sram_valid) begin
    mem[r_write_address] <= r_write_data;
  end
  // Main APB FSM
  always_ff @(posedge pclk or negedge presetn) begin
    if ((!presetn)) begin
      prdata <= 0;
      pready <= 1'b0;
      pslverr <= 1'b0;
      r_Enable <= 0;
      r_operand_1 <= 0;
      r_operand_2 <= 0;
      r_result <= 0;
      r_write_address <= 0;
      r_write_data <= 0;
      state <= 0;
    end else begin
      if (r_Enable == 1) begin
        r_result <= 8'(mem[r_operand_1] + mem[r_operand_2]);
      end else if (r_Enable == 2) begin
        r_result <= 8'(mem[r_operand_1] * mem[r_operand_2]);
      end
      if (state == 0) begin
        pready <= 1'b0;
        pslverr <= 1'b0;
        if (pselx & ~penable) begin
          if (pwrite) begin
            state <= 1;
          end else begin
            state <= 2;
            if (paddr == 0) begin
              prdata <= 8'(r_operand_1);
            end else if (paddr == 1) begin
              prdata <= 8'(r_operand_2);
            end else if (paddr == 2) begin
              prdata <= r_Enable;
            end else if (paddr == 3) begin
              prdata <= 8'(r_write_address);
            end else if (paddr == 4) begin
              prdata <= r_write_data;
            end else if (paddr == 5) begin
              prdata <= r_result;
            end else begin
              prdata <= mem[paddr];
            end
          end
        end
      end else if (state == 1) begin
        pready <= 1'b1;
        if (paddr == 0) begin
          r_operand_1 <= 10'($unsigned(pwdata));
        end else if (paddr == 1) begin
          r_operand_2 <= 10'($unsigned(pwdata));
        end else if (paddr == 2) begin
          r_Enable <= pwdata;
        end else if (paddr == 3) begin
          r_write_address <= 10'($unsigned(pwdata));
        end else if (paddr == 4) begin
          r_write_data <= pwdata;
        end else begin
          pslverr <= 1'b1;
        end
        state <= 0;
      end else if (state == 2) begin
        pready <= 1'b1;
        state <= 0;
      end
    end
  end

endmodule

