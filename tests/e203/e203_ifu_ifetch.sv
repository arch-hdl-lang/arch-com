// E203 HBirdv2 Instruction Fetch Mini-Controller (10th E203 benchmark)
// FSM managing PC generation, instruction fetch requests, and branch redirects.
// Exercises: FSM with reg/seq datapath, {a,b} concat, {N{expr}} repeat, elsif.
//
// States:
//   Idle    — wait for reset release, then start fetching
//   WaitGnt — issue memory request, wait for bus grant
//   WaitRsp — wait for memory response (instruction data)
//   Abort   — branch redirect received; cancel in-flight fetch
//
// Handshake: req_valid/req_ready to memory, rsp_valid/rsp_ready from memory.
// On branch redirect: current fetch is aborted, PC reloaded.
// domain SysDomain
//   freq_mhz: 100

module IfuIfetch #(
  parameter int XLEN = 32,
  parameter int RESET_PC = 'h80000000
) (
  input logic clk,
  input logic rst,
  output logic req_valid,
  input logic req_ready,
  output logic [32-1:0] req_addr,
  input logic rsp_valid,
  output logic rsp_ready,
  input logic [32-1:0] rsp_instr,
  input logic rsp_err,
  output logic o_valid,
  input logic o_ready,
  output logic [32-1:0] o_instr,
  output logic [32-1:0] o_pc,
  output logic o_bus_err,
  input logic redirect,
  input logic [32-1:0] redirect_pc
);

  typedef enum logic [1:0] {
    IDLE = 2'd0,
    WAITGNT = 2'd1,
    WAITRSP = 2'd2,
    ABORT = 2'd3
  } IfuIfetch_state_t;
  
  IfuIfetch_state_t state_r, state_next;
  
  logic [32-1:0] pc_r;
  logic [32-1:0] instr_r;
  logic [32-1:0] pc_out_r;
  logic bus_err_r;
  
  logic [32-1:0] pc_plus4;
  assign pc_plus4 = 32'((pc_r + 4));
  logic [32-1:0] pc_aligned;
  assign pc_aligned = {pc_plus4[31:2], {2{1'b0}}};
  
  always_ff @(posedge clk or negedge rst) begin
    if ((!rst)) begin
      state_r <= IDLE;
      pc_r <= 0;
      instr_r <= 0;
      pc_out_r <= 0;
      bus_err_r <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        IDLE: begin
          pc_r <= RESET_PC;
        end
        WAITGNT: begin
          if (redirect) begin
            pc_r <= redirect_pc;
          end
        end
        WAITRSP: begin
          if (redirect) begin
            pc_r <= redirect_pc;
          end else if (rsp_valid) begin
            instr_r <= rsp_instr;
            pc_out_r <= pc_r;
            bus_err_r <= rsp_err;
            pc_r <= pc_aligned;
          end
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      IDLE: begin
        state_next = WAITGNT;
      end
      WAITGNT: begin
        if (redirect) state_next = ABORT;
        else if (req_ready) state_next = WAITRSP;
      end
      WAITRSP: begin
        if (redirect) state_next = ABORT;
        else if (rsp_valid) state_next = WAITGNT;
      end
      ABORT: begin
        state_next = WAITGNT;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    req_valid = 1'b0; // default
    req_addr = 0; // default
    rsp_ready = 1'b0; // default
    o_valid = 1'b0; // default
    o_instr = 0; // default
    o_pc = 0; // default
    o_bus_err = 1'b0; // default
    case (state_r)
      IDLE: begin
      end
      WAITGNT: begin
        req_valid = 1'b1;
        req_addr = pc_r;
      end
      WAITRSP: begin
        rsp_ready = 1'b1;
      end
      ABORT: begin
        rsp_ready = 1'b1;
      end
      default: ;
    endcase
  end

endmodule

// Instruction memory request interface
// Instruction memory response interface
// Output to decode stage
// Branch redirect from EXU
// Datapath registers
// Next PC: PC+4 with bottom 2 bits forced to 0 (aligned)
// After reset, load reset vector and start fetching
// Issue fetch request
// Discard any in-flight response; redirect already captured
// Go back to fetching from redirected PC
