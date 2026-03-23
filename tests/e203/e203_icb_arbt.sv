// E203 ICB Bus Arbiter (2-master)
// Round-robin arbitration between two ICB masters.
// Internal Command Bus (ICB) is E203's on-chip bus protocol:
//   cmd: valid/ready + addr + wdata + wmask + read
//   rsp: valid/ready + rdata + err
module IcbArbt (
  input logic clk,
  input logic rst_n,
  input logic m0_cmd_valid,
  output logic m0_cmd_ready,
  input logic [32-1:0] m0_cmd_addr,
  input logic [32-1:0] m0_cmd_wdata,
  input logic [4-1:0] m0_cmd_wmask,
  input logic m0_cmd_read,
  output logic m0_rsp_valid,
  input logic m0_rsp_ready,
  output logic [32-1:0] m0_rsp_rdata,
  output logic m0_rsp_err,
  input logic m1_cmd_valid,
  output logic m1_cmd_ready,
  input logic [32-1:0] m1_cmd_addr,
  input logic [32-1:0] m1_cmd_wdata,
  input logic [4-1:0] m1_cmd_wmask,
  input logic m1_cmd_read,
  output logic m1_rsp_valid,
  input logic m1_rsp_ready,
  output logic [32-1:0] m1_rsp_rdata,
  output logic m1_rsp_err,
  output logic s_cmd_valid,
  input logic s_cmd_ready,
  output logic [32-1:0] s_cmd_addr,
  output logic [32-1:0] s_cmd_wdata,
  output logic [4-1:0] s_cmd_wmask,
  output logic s_cmd_read,
  input logic s_rsp_valid,
  output logic s_rsp_ready,
  input logic [32-1:0] s_rsp_rdata,
  input logic s_rsp_err
);

  // Master 0 (higher default priority)
  // Master 1
  // Slave port
  // ── Round-robin state ──────────────────────────────────────────
  logic last_grant = 1'b0;
  // false=m0, true=m1
  // ── Arbitration logic ──────────────────────────────────────────
  // If both request, alternate; otherwise grant whoever requests
  logic both_req;
  assign both_req = (m0_cmd_valid & m1_cmd_valid);
  logic grant_m0;
  assign grant_m0 = (m0_cmd_valid & ((~both_req) | last_grant));
  logic grant_m1;
  assign grant_m1 = (m1_cmd_valid & (~grant_m0));
  // Track which master owns the current outstanding transaction
  logic rsp_owner = 1'b0;
  // false=m0, true=m1
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      last_grant <= 1'b0;
      rsp_owner <= 1'b0;
    end else begin
      if ((s_cmd_valid & s_cmd_ready)) begin
        last_grant <= grant_m1;
        rsp_owner <= grant_m1;
      end
    end
  end
  // Update last_grant on successful cmd handshake
  always_comb begin
    s_cmd_valid = (m0_cmd_valid | m1_cmd_valid);
    if (grant_m0) begin
      s_cmd_addr = m0_cmd_addr;
      s_cmd_wdata = m0_cmd_wdata;
      s_cmd_wmask = m0_cmd_wmask;
      s_cmd_read = m0_cmd_read;
    end else begin
      s_cmd_addr = m1_cmd_addr;
      s_cmd_wdata = m1_cmd_wdata;
      s_cmd_wmask = m1_cmd_wmask;
      s_cmd_read = m1_cmd_read;
    end
    m0_cmd_ready = (grant_m0 & s_cmd_ready);
    m1_cmd_ready = (grant_m1 & s_cmd_ready);
    m0_rsp_valid = (s_rsp_valid & (~rsp_owner));
    m1_rsp_valid = (s_rsp_valid & rsp_owner);
    m0_rsp_rdata = s_rsp_rdata;
    m1_rsp_rdata = s_rsp_rdata;
    m0_rsp_err = s_rsp_err;
    m1_rsp_err = s_rsp_err;
    s_rsp_ready = (rsp_owner) ? (m1_rsp_ready) : (m0_rsp_ready);
  end

endmodule

// Command mux
// Command ready: only the granted master gets ready
// Response demux: route to whichever master owns the txn
// Response ready
