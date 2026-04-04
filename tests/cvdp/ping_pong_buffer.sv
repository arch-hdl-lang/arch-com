module ping_pong_buffer (
  input logic clk,
  input logic rst_n,
  input logic write_enable,
  input logic read_enable,
  input logic [8-1:0] data_in,
  output logic [8-1:0] data_out,
  output logic buffer_full,
  output logic buffer_empty,
  output logic buffer_select
);

  // Memory arrays - two banks of 255 x 8-bit  
  logic [8-1:0] mem0 [256-1:0];
  logic [8-1:0] mem1 [256-1:0];
  // Pointers
  logic [8-1:0] write_ptr;
  logic [8-1:0] read_ptr;
  // Reset pipeline: high for 2 cycles after reset deassert
  // Prevents stale write_enable from corrupting state
  logic rst_pipe0;
  logic rst_pipe1;
  logic in_reset_pipe;
  assign in_reset_pipe = rst_pipe0 || rst_pipe1;
  // Async read from memories and output mux
  always_comb begin
    if (buffer_select) begin
      data_out = mem1[read_ptr];
    end else begin
      data_out = mem0[read_ptr];
    end
  end
  // Memory write logic
  always_ff @(posedge clk) begin
    if (write_enable && !buffer_full && !in_reset_pipe) begin
      if (!buffer_select) begin
        mem0[write_ptr] <= data_in;
      end else begin
        mem1[write_ptr] <= data_in;
      end
    end
  end
  // Pointer and state management
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      buffer_empty <= 1'b1;
      buffer_full <= 1'b0;
      buffer_select <= 1'b0;
      read_ptr <= 0;
      rst_pipe0 <= 1'b1;
      rst_pipe1 <= 1'b1;
      write_ptr <= 0;
    end else begin
      if (rst_n == 1'b0) begin
        write_ptr <= 0;
        read_ptr <= 0;
        buffer_full <= 1'b0;
        buffer_empty <= 1'b1;
        buffer_select <= 1'b0;
        rst_pipe0 <= 1'b1;
        rst_pipe1 <= 1'b1;
      end else begin
        // Reset pipeline shift
        rst_pipe0 <= 1'b0;
        rst_pipe1 <= rst_pipe0;
        // Write path - blocked during reset pipeline
        if (write_enable && !buffer_full && !in_reset_pipe) begin
          buffer_empty <= 1'b0;
          if (write_ptr == 254) begin
            write_ptr <= 0;
            buffer_full <= 1'b1;
          end else begin
            write_ptr <= 8'(write_ptr + 1);
          end
        end
        // Read path
        if (read_enable && !buffer_empty) begin
          if (read_ptr == 254) begin
            read_ptr <= 0;
            buffer_select <= !buffer_select;
            buffer_empty <= 1'b1;
            buffer_full <= 1'b0;
          end else begin
            read_ptr <= 8'(read_ptr + 1);
          end
        end
      end
    end
  end

endmodule

