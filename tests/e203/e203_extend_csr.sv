// E203 Extended CSR (NICE Co-processor CSR stub)
// Placeholder for custom NICE accelerator CSR registers.
// Always ready, reads return zero.
module e203_extend_csr (
  input logic clk,
  input logic rst_n,
  input logic nice_csr_valid,
  output logic nice_csr_ready,
  input logic [32-1:0] nice_csr_addr,
  input logic nice_csr_wr,
  input logic [32-1:0] nice_csr_wdata,
  output logic [32-1:0] nice_csr_rdata
);

  assign nice_csr_ready = 1'b1;
  assign nice_csr_rdata = 0;

endmodule

