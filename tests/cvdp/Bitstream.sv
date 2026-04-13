module Bitstream (
  input logic clk,
  input logic rst_n,
  input logic enb,
  input logic rempty_in,
  input logic rinc_in,
  input logic [7:0] i_byte,
  output logic o_bit,
  output logic rempty_out,
  output logic rinc_out,
  output logic [1:0] curr_state
);

  // State register: 0=IDLE, 1=WaitR, 2=Ready
  logic [1:0] state_r;
  // Data registers
  logic [3:0] bp;
  logic [7:0] byte_buf;
  assign curr_state = state_r;
  // rde: read-done flag (bp[3]=1 means all 8 bits consumed)
  logic rde;
  assign rde = bp[3];
  // Combinatorial outputs based on CURRENT state and inputs
  logic rinc_out_c;
  logic rempty_out_c;
  always_comb begin
    if (state_r == 0) begin
      // IDLE
      rinc_out_c = 1'b0;
      rempty_out_c = 1'b1;
    end else if (state_r == 1) begin
      // WaitR
      rempty_out_c = 1'b1;
      if (rempty_in) begin
        rinc_out_c = 1'b0;
      end else begin
        rinc_out_c = 1'b1;
      end
    end else if (state_r == 2) begin
      // Ready
      if (rde) begin
        if (rempty_in) begin
          rinc_out_c = 1'b0;
          rempty_out_c = 1'b1;
        end else begin
          rinc_out_c = 1'b1;
          rempty_out_c = 1'b1;
        end
      end else begin
        rinc_out_c = 1'b0;
        rempty_out_c = 1'b0;
      end
    end else begin
      rinc_out_c = 1'b0;
      rempty_out_c = 1'b1;
    end
    rinc_out = rinc_out_c;
    rempty_out = rempty_out_c;
    // o_bit: combinatorial from current byte_buf and bp
    o_bit = byte_buf[bp[2:0]];
  end
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      bp <= 0;
      byte_buf <= 0;
      state_r <= 0;
    end else begin
      // State machine transitions
      if (state_r == 0) begin
        // IDLE
        if (enb) begin
          state_r <= 1;
        end
      end else if (state_r == 1) begin
        // WaitR
        if (~rempty_in) begin
          state_r <= 2;
        end
      end else if (state_r == 2) begin
        // Ready
        if (rde & rempty_in) begin
          state_r <= 1;
        end
      end
      // Update data registers based on combinatorial rinc_out
      if (rinc_out_c) begin
        byte_buf <= i_byte;
        bp <= 0;
      end else if (rinc_in & ~rempty_out_c) begin
        bp <= 4'(bp + 4'd1);
      end
    end
  end

endmodule

