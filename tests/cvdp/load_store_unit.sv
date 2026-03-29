module load_store_unit (
  input logic clk,
  input logic rst_n,
  output logic dmem_req_o,
  output logic [32-1:0] dmem_req_addr_o,
  output logic dmem_req_we_o,
  output logic [4-1:0] dmem_req_be_o,
  output logic [32-1:0] dmem_req_wdata_o,
  input logic dmem_gnt_i,
  input logic dmem_rvalid_i,
  input logic [32-1:0] dmem_rsp_rdata_i,
  input logic ex_if_req_i,
  input logic ex_if_we_i,
  input logic [2-1:0] ex_if_type_i,
  input logic [32-1:0] ex_if_wdata_i,
  input logic [32-1:0] ex_if_addr_base_i,
  input logic [32-1:0] ex_if_addr_offset_i,
  output logic ex_if_ready_o,
  output logic [32-1:0] wb_if_rdata_o,
  output logic wb_if_rvalid_o
);

  // Data-cache interface
  // Execute stage interface
  // Writeback interface
  // FSM states: 0=IDLE, 1=WAIT_GNT, 2=WAIT_RVALID
  logic [2-1:0] state;
  logic is_store;
  // Combinational address calculation
  logic [32-1:0] addr;
  assign addr = 32'(ex_if_addr_base_i + ex_if_addr_offset_i);
  logic [2-1:0] addr_lsb;
  assign addr_lsb = addr[1:0];
  // Combinational byte enable and misaligned detection
  logic [4-1:0] be;
  logic misaligned;
  // Accept condition: combine into single wire to avoid && codegen issue
  logic accept;
  assign accept = ex_if_req_i & ex_if_ready_o & misaligned == 1'b0;
  always_comb begin
    if (ex_if_type_i == 0) begin
      misaligned = 1'b0;
      if (addr_lsb == 0) begin
        be = 1;
      end else if (addr_lsb == 1) begin
        be = 2;
      end else if (addr_lsb == 2) begin
        be = 4;
      end else begin
        be = 8;
      end
    end else if (ex_if_type_i == 1) begin
      if (addr_lsb == 0) begin
        be = 3;
        misaligned = 1'b0;
      end else if (addr_lsb == 2) begin
        be = 12;
        misaligned = 1'b0;
      end else begin
        be = 0;
        misaligned = 1'b1;
      end
    end else if (ex_if_type_i == 2) begin
      if (addr_lsb == 0) begin
        be = 15;
        misaligned = 1'b0;
      end else begin
        be = 0;
        misaligned = 1'b1;
      end
    end else begin
      be = 0;
      misaligned = 1'b1;
    end
  end
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      dmem_req_addr_o <= 0;
      dmem_req_be_o <= 0;
      dmem_req_o <= 1'b0;
      dmem_req_wdata_o <= 0;
      dmem_req_we_o <= 1'b0;
      ex_if_ready_o <= 1'b1;
      is_store <= 1'b0;
      state <= 0;
      wb_if_rdata_o <= 0;
      wb_if_rvalid_o <= 1'b0;
    end else begin
      if (state == 0) begin
        wb_if_rvalid_o <= 1'b0;
        if (accept) begin
          dmem_req_o <= 1'b1;
          dmem_req_addr_o <= addr;
          dmem_req_we_o <= ex_if_we_i;
          dmem_req_be_o <= be;
          dmem_req_wdata_o <= ex_if_wdata_i;
          ex_if_ready_o <= 1'b0;
          is_store <= ex_if_we_i;
          state <= 1;
        end
      end else if (state == 1) begin
        wb_if_rvalid_o <= 1'b0;
        if (dmem_gnt_i) begin
          dmem_req_o <= 1'b0;
          dmem_req_we_o <= 1'b0;
          dmem_req_addr_o <= 0;
          dmem_req_be_o <= 0;
          dmem_req_wdata_o <= 0;
          if (is_store) begin
            ex_if_ready_o <= 1'b1;
            state <= 0;
          end else begin
            state <= 2;
          end
        end
      end else if (state == 2) begin
        if (dmem_rvalid_i) begin
          wb_if_rvalid_o <= 1'b1;
          wb_if_rdata_o <= dmem_rsp_rdata_i;
          ex_if_ready_o <= 1'b1;
          state <= 0;
        end else begin
          wb_if_rvalid_o <= 1'b0;
        end
      end else begin
        state <= 0;
      end
    end
  end

endmodule

