module apb_controller (
  input logic clk,
  input logic reset_n,
  input logic select_a_i,
  input logic select_b_i,
  input logic select_c_i,
  input logic [32-1:0] addr_a_i,
  input logic [32-1:0] data_a_i,
  input logic [32-1:0] addr_b_i,
  input logic [32-1:0] data_b_i,
  input logic [32-1:0] addr_c_i,
  input logic [32-1:0] data_c_i,
  input logic apb_pready_i,
  output logic apb_psel_o,
  output logic apb_penable_o,
  output logic apb_pwrite_o,
  output logic [32-1:0] apb_paddr_o,
  output logic [32-1:0] apb_pwdata_o
);

  // FSM states: 0=IDLE, 1=SETUP, 2=ACCESS
  logic [2-1:0] state;
  logic r_psel;
  logic r_penable;
  logic r_pwrite;
  logic [32-1:0] r_paddr;
  logic [32-1:0] r_pwdata;
  logic [4-1:0] timeout_cnt;
  always_ff @(posedge clk or negedge reset_n) begin
    if ((!reset_n)) begin
      r_paddr <= 0;
      r_penable <= 1'b0;
      r_psel <= 1'b0;
      r_pwdata <= 0;
      r_pwrite <= 1'b0;
      state <= 0;
      timeout_cnt <= 0;
    end else begin
      if (state == 0) begin
        if (select_a_i) begin
          r_psel <= 1'b1;
          r_pwrite <= 1'b1;
          r_paddr <= addr_a_i;
          r_pwdata <= data_a_i;
          r_penable <= 1'b0;
          state <= 1;
        end else if (select_b_i) begin
          r_psel <= 1'b1;
          r_pwrite <= 1'b1;
          r_paddr <= addr_b_i;
          r_pwdata <= data_b_i;
          r_penable <= 1'b0;
          state <= 1;
        end else if (select_c_i) begin
          r_psel <= 1'b1;
          r_pwrite <= 1'b1;
          r_paddr <= addr_c_i;
          r_pwdata <= data_c_i;
          r_penable <= 1'b0;
          state <= 1;
        end else begin
          r_psel <= 1'b0;
          r_penable <= 1'b0;
          r_pwrite <= 1'b0;
          r_paddr <= 0;
          r_pwdata <= 0;
        end
        timeout_cnt <= 0;
      end else if (state == 1) begin
        r_penable <= 1'b1;
        state <= 2;
      end else if (state == 2) begin
        if (apb_pready_i) begin
          r_psel <= 1'b0;
          r_penable <= 1'b0;
          r_pwrite <= 1'b0;
          r_paddr <= 0;
          r_pwdata <= 0;
          timeout_cnt <= 0;
          state <= 0;
        end else if (timeout_cnt == 15) begin
          r_psel <= 1'b0;
          r_penable <= 1'b0;
          r_pwrite <= 1'b0;
          r_paddr <= 0;
          r_pwdata <= 0;
          timeout_cnt <= 0;
          state <= 0;
        end else begin
          timeout_cnt <= 4'(timeout_cnt + 1);
        end
      end else begin
        state <= 0;
      end
    end
  end
  // IDLE: check for events with priority A > B > C
  // SETUP: assert penable, move to ACCESS
  // ACCESS: wait for pready or timeout
  // Transaction complete, return to IDLE
  // Timeout: abort, return to IDLE
  assign apb_psel_o = r_psel;
  assign apb_penable_o = r_penable;
  assign apb_pwrite_o = r_pwrite;
  assign apb_paddr_o = r_paddr;
  assign apb_pwdata_o = r_pwdata;

endmodule

