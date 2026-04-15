module memory_block (
  input logic axi_clk,
  input logic ctrld_clk,
  input logic reset_in,
  input logic we,
  input logic [3:0] write_address,
  input logic [31:0] write_data,
  input logic [3:0] address_a,
  input logic [3:0] address_b,
  input logic [3:0] address_c,
  output logic [31:0] data_a,
  output logic [31:0] data_b,
  output logic [31:0] data_c,
  output logic [31:0] result_address
);

  logic [15:0] [31:0] mem;
  logic [31:0] result_address_r;
  // Synchronous write on axi_clk
  always_ff @(posedge axi_clk) begin
    if (reset_in) begin
      for (int __ri0 = 0; __ri0 < 16; __ri0++) begin
        mem[__ri0] <= 0;
      end
      result_address_r <= 0;
    end else begin
      if (we) begin
        mem[write_address] <= write_data;
        if (write_address == 0) begin
          result_address_r <= write_data;
        end
      end
    end
  end
  // Asynchronous read (combinational)
  assign data_a = mem[address_a];
  assign data_b = mem[address_b];
  assign data_c = mem[address_c];
  assign result_address = result_address_r;

endmodule

module axi_alu (
  input logic axi_clk_in,
  input logic fast_clk_in,
  input logic reset_in,
  input logic [31:0] axi_awaddr_i,
  input logic [7:0] axi_awlen_i,
  input logic [2:0] axi_awsize_i,
  input logic [1:0] axi_awburst_i,
  input logic axi_awvalid_i,
  output logic axi_awready_o,
  input logic [31:0] axi_wdata_i,
  input logic [3:0] axi_wstrb_i,
  input logic axi_wlast_i,
  input logic axi_wvalid_i,
  output logic axi_wready_o,
  output logic axi_bvalid_o,
  output logic [1:0] axi_bresp_o,
  input logic axi_bready_i,
  input logic [31:0] axi_araddr_i,
  input logic [7:0] axi_arlen_i,
  input logic [2:0] axi_arsize_i,
  input logic [1:0] axi_arburst_i,
  input logic axi_arvalid_i,
  output logic axi_arready_o,
  output logic [31:0] axi_rdata_o,
  output logic axi_rvalid_o,
  output logic [1:0] axi_rresp_o,
  output logic axi_rlast_o,
  input logic axi_rready_i,
  output logic [63:0] result_o
);

  // AXI Write Address Channel
  // AXI Write Data Channel
  // AXI Write Response Channel
  // AXI Read Address Channel
  // AXI Read Data Channel
  // DSP result output
  // CSR registers
  logic [31:0] operand_a_addr;
  logic [31:0] operand_b_addr;
  logic [31:0] operand_c_addr;
  logic [1:0] op_select_reg;
  logic start_reg;
  logic clock_control;
  // AXI Write state
  logic [31:0] aw_addr;
  logic [7:0] aw_len;
  logic [2:0] aw_size;
  logic [1:0] aw_burst;
  logic awready_r;
  logic wready_r;
  logic bvalid_r;
  logic [7:0] wr_beat_cnt;
  logic wr_active;
  // AXI Read state
  logic [31:0] ar_addr;
  logic [7:0] ar_len;
  logic [2:0] ar_size;
  logic [1:0] ar_burst;
  logic arready_r;
  logic rvalid_r;
  logic [31:0] rdata_r;
  logic rlast_r;
  logic [7:0] rd_beat_cnt;
  logic rd_active;
  // Memory interface signals
  logic mem_we;
  logic [3:0] mem_waddr;
  logic [31:0] mem_wdata;
  // CDC synchronizers (double-flop for CSR -> DSP domain)
  logic [31:0] cdc_op_a_addr_s1;
  logic [31:0] cdc_op_a_addr_s2;
  logic [31:0] cdc_op_b_addr_s1;
  logic [31:0] cdc_op_b_addr_s2;
  logic [31:0] cdc_op_c_addr_s1;
  logic [31:0] cdc_op_c_addr_s2;
  logic [1:0] cdc_op_sel_s1;
  logic [1:0] cdc_op_sel_s2;
  logic cdc_start_s1;
  logic cdc_start_s2;
  // CDC synchronizers (DSP result -> AXI domain)
  logic [63:0] cdc_result_s1;
  logic [63:0] cdc_result_s2;
  // DSP block registers (fast_clk domain)
  logic [63:0] dsp_result_r;
  // Memory block output wires
  logic [31:0] mem_data_a;
  logic [31:0] mem_data_b;
  logic [31:0] mem_data_c;
  logic [31:0] mem_result_addr;
  // Wires for address calculations
  logic [31:0] wr_addr_word;
  logic [31:0] rd_addr_word;
  logic [31:0] wr_addr_next;
  logic [31:0] rd_addr_next;
  // DSP input wires (after CDC mux)
  logic [3:0] dsp_op_a_addr;
  logic [3:0] dsp_op_b_addr;
  logic [3:0] dsp_op_c_addr;
  logic [1:0] dsp_op_sel;
  logic dsp_start;
  // DSP result wire
  logic [63:0] dsp_result_val;
  // Write in-memory-range flag
  logic wr_in_mem;
  // Combinational logic
  assign wr_addr_word = aw_addr >> 2;
  assign rd_addr_word = ar_addr >> 2;
  assign wr_addr_next = 32'(aw_addr + (32'($unsigned(1)) << 32'($unsigned(aw_size))));
  assign rd_addr_next = 32'(ar_addr + (32'($unsigned(1)) << 32'($unsigned(ar_size))));
  assign wr_in_mem = wr_addr_word >= 8;
  assign axi_awready_o = awready_r;
  assign axi_wready_o = wready_r;
  assign axi_bvalid_o = bvalid_r;
  assign axi_bresp_o = 0;
  assign axi_arready_o = arready_r;
  assign axi_rdata_o = rdata_r;
  assign axi_rvalid_o = rvalid_r;
  assign axi_rresp_o = 0;
  assign axi_rlast_o = rlast_r;
  assign dsp_op_a_addr = clock_control ? cdc_op_a_addr_s2[3:0] : operand_a_addr[3:0];
  assign dsp_op_b_addr = clock_control ? cdc_op_b_addr_s2[3:0] : operand_b_addr[3:0];
  assign dsp_op_c_addr = clock_control ? cdc_op_c_addr_s2[3:0] : operand_c_addr[3:0];
  assign dsp_op_sel = clock_control ? cdc_op_sel_s2 : op_select_reg;
  assign dsp_start = clock_control ? cdc_start_s2 : start_reg;
  assign dsp_result_val = clock_control ? cdc_result_s2 : dsp_result_r;
  assign result_o = dsp_result_val;
  // Memory block instance
  memory_block u_memory_block (
    .axi_clk(axi_clk_in),
    .ctrld_clk(fast_clk_in),
    .reset_in(reset_in),
    .we(mem_we),
    .write_address(mem_waddr),
    .write_data(mem_wdata),
    .address_a(dsp_op_a_addr),
    .address_b(dsp_op_b_addr),
    .address_c(dsp_op_c_addr),
    .data_a(mem_data_a),
    .data_b(mem_data_b),
    .data_c(mem_data_c),
    .result_address(mem_result_addr)
  );
  // AXI Write Channel (unified seq block)
  always_ff @(posedge axi_clk_in) begin
    if (reset_in) begin
      aw_addr <= 0;
      aw_burst <= 0;
      aw_len <= 0;
      aw_size <= 0;
      awready_r <= 1'b1;
      bvalid_r <= 1'b0;
      clock_control <= 1'b0;
      mem_waddr <= 0;
      mem_wdata <= 0;
      mem_we <= 1'b0;
      op_select_reg <= 0;
      operand_a_addr <= 0;
      operand_b_addr <= 0;
      operand_c_addr <= 0;
      start_reg <= 1'b0;
      wr_active <= 1'b0;
      wr_beat_cnt <= 0;
      wready_r <= 1'b0;
    end else begin
      mem_we <= 1'b0;
      // Write address handshake
      if (axi_awvalid_i & awready_r) begin
        aw_addr <= axi_awaddr_i;
        aw_len <= axi_awlen_i;
        aw_size <= axi_awsize_i;
        aw_burst <= axi_awburst_i;
        awready_r <= 1'b0;
        wr_active <= 1'b1;
        wr_beat_cnt <= 0;
      end
      // Assert wready when write active and not already processing
      if (wr_active & ~wready_r & ~bvalid_r) begin
        wready_r <= 1'b1;
      end
      // Write data handling
      if (axi_wvalid_i & wready_r) begin
        if (wr_addr_word < 8) begin
          if (wr_addr_word == 0) begin
            operand_a_addr <= axi_wdata_i;
          end else if (wr_addr_word == 1) begin
            operand_b_addr <= axi_wdata_i;
          end else if (wr_addr_word == 2) begin
            operand_c_addr <= axi_wdata_i;
          end else if (wr_addr_word == 3) begin
            op_select_reg <= axi_wdata_i[1:0];
            start_reg <= axi_wdata_i[2];
          end else if (wr_addr_word == 4) begin
            clock_control <= axi_wdata_i[0];
          end
        end else if (wr_in_mem) begin
          mem_we <= 1'b1;
          mem_waddr <= 4'(wr_addr_word - 8);
          mem_wdata <= axi_wdata_i;
        end
        // Burst address increment
        if (aw_burst == 1) begin
          aw_addr <= wr_addr_next;
        end
        if (axi_wlast_i | (wr_beat_cnt == aw_len)) begin
          wready_r <= 1'b0;
          bvalid_r <= 1'b1;
          wr_active <= 1'b0;
          wr_beat_cnt <= 0;
        end else begin
          wr_beat_cnt <= 8'(wr_beat_cnt + 1);
        end
      end
      // Write response handshake
      if (bvalid_r & axi_bready_i) begin
        bvalid_r <= 1'b0;
        awready_r <= 1'b1;
      end
    end
  end
  // AXI Read Channel (unified seq block)
  always_ff @(posedge axi_clk_in) begin
    if (reset_in) begin
      ar_addr <= 0;
      ar_burst <= 0;
      ar_len <= 0;
      ar_size <= 0;
      arready_r <= 1'b1;
      rd_active <= 1'b0;
      rd_beat_cnt <= 0;
      rdata_r <= 0;
      rlast_r <= 1'b0;
      rvalid_r <= 1'b0;
    end else begin
      if (axi_arvalid_i & arready_r) begin
        ar_addr <= axi_araddr_i;
        ar_len <= axi_arlen_i;
        ar_size <= axi_arsize_i;
        ar_burst <= axi_arburst_i;
        arready_r <= 1'b0;
        rd_active <= 1'b1;
        rd_beat_cnt <= 0;
      end
      if (rd_active & ~rvalid_r) begin
        if (rd_addr_word == 0) begin
          rdata_r <= operand_a_addr;
        end else if (rd_addr_word == 1) begin
          rdata_r <= operand_b_addr;
        end else if (rd_addr_word == 2) begin
          rdata_r <= operand_c_addr;
        end else if (rd_addr_word == 3) begin
          rdata_r <= {29'd0, start_reg, op_select_reg};
        end else if (rd_addr_word == 4) begin
          rdata_r <= {31'd0, clock_control};
        end else if (rd_addr_word == 5) begin
          rdata_r <= 32'(dsp_result_val);
        end else if (rd_addr_word == 6) begin
          rdata_r <= 32'(dsp_result_val >> 32);
        end else begin
          rdata_r <= 0;
        end
        rvalid_r <= 1'b1;
        if (rd_beat_cnt == ar_len) begin
          rlast_r <= 1'b1;
        end else begin
          rlast_r <= 1'b0;
        end
      end
      if (rvalid_r & axi_rready_i) begin
        rvalid_r <= 1'b0;
        if (rlast_r) begin
          rlast_r <= 1'b0;
          rd_active <= 1'b0;
          arready_r <= 1'b1;
        end else begin
          rd_beat_cnt <= 8'(rd_beat_cnt + 1);
          if (ar_burst == 1) begin
            ar_addr <= rd_addr_next;
          end
        end
      end
    end
  end
  // CDC: AXI clock -> Fast clock (double-flop synchronizer, always running)
  always_ff @(posedge fast_clk_in) begin
    if (reset_in) begin
      cdc_op_a_addr_s1 <= 0;
      cdc_op_a_addr_s2 <= 0;
      cdc_op_b_addr_s1 <= 0;
      cdc_op_b_addr_s2 <= 0;
      cdc_op_c_addr_s1 <= 0;
      cdc_op_c_addr_s2 <= 0;
      cdc_op_sel_s1 <= 0;
      cdc_op_sel_s2 <= 0;
      cdc_start_s1 <= 1'b0;
      cdc_start_s2 <= 1'b0;
    end else begin
      cdc_op_a_addr_s1 <= operand_a_addr;
      cdc_op_a_addr_s2 <= cdc_op_a_addr_s1;
      cdc_op_b_addr_s1 <= operand_b_addr;
      cdc_op_b_addr_s2 <= cdc_op_b_addr_s1;
      cdc_op_c_addr_s1 <= operand_c_addr;
      cdc_op_c_addr_s2 <= cdc_op_c_addr_s1;
      cdc_op_sel_s1 <= op_select_reg;
      cdc_op_sel_s2 <= cdc_op_sel_s1;
      cdc_start_s1 <= start_reg;
      cdc_start_s2 <= cdc_start_s1;
    end
  end
  // DSP Block: compute result on fast_clk with CDC-muxed inputs
  always_ff @(posedge fast_clk_in) begin
    if (reset_in) begin
      dsp_result_r <= 0;
    end else begin
      if (dsp_start) begin
        if (dsp_op_sel == 0) begin
          dsp_result_r <= 64'((64'($unsigned(mem_data_a)) + 64'($unsigned(mem_data_b))) * 64'($unsigned(mem_data_c)));
        end else if (dsp_op_sel == 1) begin
          dsp_result_r <= 64'(64'($unsigned(mem_data_a)) * 64'($unsigned(mem_data_b)));
        end else if (dsp_op_sel == 2) begin
          dsp_result_r <= 64'($unsigned(mem_data_a)) >> 64'($unsigned(mem_data_b[4:0]));
        end else if (dsp_op_sel == 3) begin
          if (mem_data_b != 0) begin
            dsp_result_r <= 64'($unsigned(mem_data_a)) / 64'($unsigned(mem_data_b));
          end else begin
            dsp_result_r <= 64'd3735936685;
          end
        end
      end
    end
  end
  // CDC: Fast clock -> AXI clock (double-flop synchronizer, always running)
  always_ff @(posedge axi_clk_in) begin
    if (reset_in) begin
      cdc_result_s1 <= 0;
      cdc_result_s2 <= 0;
    end else begin
      cdc_result_s1 <= dsp_result_r;
      cdc_result_s2 <= cdc_result_s1;
    end
  end

endmodule

