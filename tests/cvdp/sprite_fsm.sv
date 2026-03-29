module sprite_controller_fsm #(
  parameter int MEM_ADDR_WIDTH = 16,
  parameter int PIXEL_WIDTH = 24,
  parameter int SPRITE_WIDTH = 16,
  parameter int SPRITE_HEIGHT = 16,
  parameter int WAIT_WIDTH = 4,
  parameter int N_ROM = 256
) (
  input logic clk,
  input logic rst_n,
  input logic [WAIT_WIDTH-1:0] i_wait,
  output logic rw,
  output logic [MEM_ADDR_WIDTH-1:0] write_addr,
  output logic [PIXEL_WIDTH-1:0] write_data,
  output logic [SPRITE_WIDTH-1:0] x_pos,
  output logic [SPRITE_HEIGHT-1:0] y_pos,
  input logic [PIXEL_WIDTH-1:0] pixel_out,
  output logic done
);

  // State encoding: IDLE=0, INIT_WRITE=1, WRITE=2, INIT_READ=3, READ=4, WAIT=5, DONE=6
  logic [3-1:0] st;
  logic [MEM_ADDR_WIDTH-1:0] addr_cnt;
  logic [PIXEL_WIDTH-1:0] data_cnt;
  logic [WAIT_WIDTH-1:0] wait_cnt;
  always_ff @(posedge clk) begin
    if ((!rst_n)) begin
      addr_cnt <= 0;
      data_cnt <= 0;
      done <= 1'b0;
      rw <= 1'b0;
      st <= 0;
      wait_cnt <= 0;
      write_addr <= 0;
      write_data <= 0;
      x_pos <= 0;
      y_pos <= 0;
    end else begin
      if (st == 0) begin
        // IDLE
        rw <= 1'b0;
        addr_cnt <= 0;
        data_cnt <= 0;
        done <= 1'b0;
        st <= 1;
      end else if (st == 1) begin
        // INIT_WRITE
        rw <= 1'b1;
        write_data <= 24'd16711680;
        write_addr <= addr_cnt;
        st <= 2;
      end else if (st == 2) begin
        // WRITE
        write_addr <= addr_cnt;
        write_data <= data_cnt;
        addr_cnt <= MEM_ADDR_WIDTH'(addr_cnt + 1);
        data_cnt <= PIXEL_WIDTH'(data_cnt + 1);
        if (32'($unsigned(addr_cnt)) == N_ROM - 1) begin
          st <= 3;
        end
      end else if (st == 3) begin
        // INIT_READ
        rw <= 1'b0;
        addr_cnt <= 0;
        st <= 4;
      end else if (st == 4) begin
        // READ: compute x/y position and advance address counter; do NOT drive write_addr
        x_pos <= SPRITE_WIDTH'(32'($unsigned(addr_cnt)) % SPRITE_WIDTH);
        y_pos <= SPRITE_HEIGHT'(32'($unsigned(addr_cnt)) / SPRITE_WIDTH);
        addr_cnt <= MEM_ADDR_WIDTH'(addr_cnt + 1);
        if (32'($unsigned(addr_cnt)) == SPRITE_WIDTH * SPRITE_HEIGHT - 1) begin
          st <= 5;
        end
      end else if (st == 5) begin
        // WAIT
        wait_cnt <= WAIT_WIDTH'(wait_cnt + 1);
        if (wait_cnt == i_wait) begin
          st <= 6;
        end
      end else if (st == 6) begin
        // DONE
        done <= 1'b1;
        st <= 0;
      end
    end
  end

endmodule

