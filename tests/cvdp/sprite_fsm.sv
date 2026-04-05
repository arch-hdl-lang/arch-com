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

  typedef enum logic [2:0] {
    IDLE = 3'd0,
    INIT_WRITE = 3'd1,
    WRITE = 3'd2,
    INIT_READ = 3'd3,
    READ = 3'd4,
    WAIT = 3'd5,
    DONE = 3'd6
  } sprite_controller_fsm_state_t;
  
  sprite_controller_fsm_state_t state_r, state_next;
  
  logic [MEM_ADDR_WIDTH-1:0] addr_cnt;
  logic [PIXEL_WIDTH-1:0] data_cnt;
  logic [WAIT_WIDTH-1:0] wait_cnt;
  logic cold_start;
  
  always_ff @(posedge clk) begin
    if ((!rst_n)) begin
      state_r <= IDLE;
      addr_cnt <= 0;
      data_cnt <= 0;
      wait_cnt <= 0;
      cold_start <= 1'b1;
    end else begin
      state_r <= state_next;
      case (state_r)
        IDLE: begin
          rw <= 1'b0;
          addr_cnt <= 0;
          data_cnt <= 0;
          done <= 1'b0;
          if (cold_start) begin
            write_addr <= 0;
            write_data <= 0;
            x_pos <= 0;
            y_pos <= 0;
            cold_start <= 1'b0;
          end
        end
        INIT_WRITE: begin
          rw <= 1'b1;
          write_data <= 24'd16711680;
          write_addr <= addr_cnt;
        end
        WRITE: begin
          write_addr <= addr_cnt;
          write_data <= data_cnt;
          addr_cnt <= MEM_ADDR_WIDTH'(addr_cnt + 1);
          data_cnt <= PIXEL_WIDTH'(data_cnt + 1);
        end
        INIT_READ: begin
          rw <= 1'b0;
          addr_cnt <= 0;
        end
        READ: begin
          x_pos <= SPRITE_WIDTH'(32'($unsigned(addr_cnt)) % SPRITE_WIDTH);
          y_pos <= SPRITE_HEIGHT'(32'($unsigned(addr_cnt)) / SPRITE_WIDTH);
          addr_cnt <= MEM_ADDR_WIDTH'(addr_cnt + 1);
        end
        WAIT: begin
          wait_cnt <= WAIT_WIDTH'(wait_cnt + 1);
        end
        DONE: begin
          done <= 1'b1;
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      IDLE: begin
        state_next = INIT_WRITE;
      end
      INIT_WRITE: begin
        state_next = WRITE;
      end
      WRITE: begin
        if (32'($unsigned(addr_cnt)) == N_ROM - 1) state_next = INIT_READ;
      end
      INIT_READ: begin
        state_next = READ;
      end
      READ: begin
        if (32'($unsigned(addr_cnt)) == SPRITE_WIDTH * SPRITE_HEIGHT - 1) state_next = WAIT;
      end
      WAIT: begin
        if (wait_cnt == i_wait) state_next = DONE;
      end
      DONE: begin
        state_next = IDLE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    case (state_r)
      IDLE: begin
      end
      INIT_WRITE: begin
      end
      WRITE: begin
      end
      INIT_READ: begin
      end
      READ: begin
      end
      WAIT: begin
      end
      DONE: begin
      end
      default: ;
    endcase
  end

endmodule

