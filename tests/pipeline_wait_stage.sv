// Pipeline with a variable-latency DataAccess stage using do..until.
// The DataAccess stage drives mem_req while waiting for mem_valid.
// domain SysDomain
//   freq_mhz: 100

module WaitPipe (
  input logic clk,
  input logic rst,
  input logic [31:0] addr_in,
  output logic [31:0] data_out,
  input logic mem_valid,
  input logic [31:0] mem_data
);

  // ── Stage valid registers ──
  logic fetch_valid_r;
  logic dataaccess_valid_r;
  logic writeback_valid_r;
  
  // ── Stage data registers ──
  logic [31:0] fetch_addr = 0;
  logic [31:0] dataaccess_data = 0;
  logic [31:0] writeback_result = 0;
  
  // ── Wait-stage FSM registers ──
  logic [0:0] dataaccess_fsm_state;
  logic dataaccess_fsm_busy;
  
  // ── Stall signals ──
  logic fetch_stall;
  logic dataaccess_stall;
  logic writeback_stall;
  assign writeback_stall = 1'b0;
  assign dataaccess_stall = dataaccess_fsm_busy || writeback_stall;
  assign fetch_stall = dataaccess_stall;
  
  assign dataaccess_fsm_busy = (dataaccess_fsm_state != '0);
  
  // ── Stage register updates ──
  always_ff @(posedge clk) begin
    if (rst) begin
      fetch_valid_r <= 1'b0;
      fetch_addr <= 0;
      dataaccess_valid_r <= 1'b0;
      dataaccess_fsm_state <= '0;
      dataaccess_data <= 0;
      writeback_valid_r <= 1'b0;
      writeback_result <= 0;
    end else begin
      if (!fetch_stall) begin
        fetch_valid_r <= 1'b1;
        fetch_addr <= addr_in;
      end
      // Wait-stage FSM: dataaccess
      case (dataaccess_fsm_state)
        1'd0: begin
          if (fetch_valid_r) begin
            if (mem_valid) begin
              dataaccess_data <= mem_data;
              dataaccess_valid_r <= fetch_valid_r;
            end else begin
              dataaccess_fsm_state <= 1'd1;
            end
          end
        end
        1'd1: begin
          if (mem_valid) begin
            dataaccess_data <= mem_data;
            dataaccess_fsm_state <= '0;
            dataaccess_valid_r <= 1'b1;
          end
        end
        default: begin
          dataaccess_fsm_state <= '0;
        end
      endcase
      if (!writeback_stall) begin
        writeback_valid_r <= dataaccess_stall ? 1'b0 : dataaccess_valid_r;
        writeback_result <= dataaccess_data;
      end
    end
  end
  
  // ── Combinational outputs ──
  assign data_out = writeback_result;

endmodule

