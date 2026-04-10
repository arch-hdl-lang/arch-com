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

  typedef enum logic [1:0] {
    IDLE = 2'd0,
    WRITEACCESS = 2'd1,
    READACCESS = 2'd2
  } apb_dsp_unit_state_t;
  
  apb_dsp_unit_state_t state_r, state_next;
  
  logic [10-1:0] r_operand_1;
  logic [10-1:0] r_operand_2;
  logic [8-1:0] r_Enable;
  logic [10-1:0] r_write_address;
  logic [8-1:0] r_write_data;
  logic [8-1:0] mem [1024-1:0];
  logic [8-1:0] r_result;
  
  always_ff @(posedge pclk or negedge presetn) begin
    if ((!presetn)) begin
      state_r <= IDLE;
      r_operand_1 <= 0;
      r_operand_2 <= 0;
      r_Enable <= 0;
      r_write_address <= 0;
      r_write_data <= 0;
      for (int __ri_mem = 0; __ri_mem < 1024; __ri_mem++) begin
        mem[__ri_mem] <= 0;
      end
      r_result <= 0;
    end else begin
      state_r <= state_next;
      // Config registers
      // SRAM 1KB - init only
      // Result register at address 0x5
      // SRAM write on posedge sram_valid
      mem[r_write_address] <= r_write_data;
      // DSP computation runs every cycle
      if (r_Enable == 1) begin
        r_result <= 8'(mem[r_operand_1] + mem[r_operand_2]);
      end else if (r_Enable == 2) begin
        r_result <= 8'(mem[r_operand_1] * mem[r_operand_2]);
      end
      case (state_r)
        IDLE: begin
          pready <= 1'b0;
          pslverr <= 1'b0;
          if (pselx & ~penable) begin
            if (~pwrite) begin
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
        end
        WRITEACCESS: begin
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
        end
        READACCESS: begin
          pready <= 1'b1;
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      IDLE: begin
        if (pselx & ~penable & pwrite) state_next = WRITEACCESS;
        else if (pselx & ~penable & ~pwrite) state_next = READACCESS;
      end
      WRITEACCESS: begin
        state_next = IDLE;
      end
      READACCESS: begin
        state_next = IDLE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    case (state_r)
      IDLE: begin
      end
      WRITEACCESS: begin
      end
      READACCESS: begin
      end
      default: ;
    endcase
  end

endmodule

