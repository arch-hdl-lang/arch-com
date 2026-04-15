module ttc_counter_lite (
  input logic clk,
  input logic reset,
  input logic [3:0] axi_addr,
  input logic [31:0] axi_wdata,
  input logic axi_write_en,
  input logic axi_read_en,
  output logic [31:0] axi_rdata,
  output logic interrupt
);

  logic [31:0] count;
  logic [31:0] match_value;
  logic [31:0] reload_value;
  logic enable;
  logic interval_mode;
  logic interrupt_enable;
  logic match_flag;
  logic status_clear;
  assign status_clear = axi_write_en & (axi_addr == 4);
  // Main sequential logic
  always_ff @(posedge clk) begin
    if (reset) begin
      count <= 0;
      enable <= 0;
      interrupt <= 0;
      interrupt_enable <= 0;
      interval_mode <= 0;
      match_flag <= 0;
      match_value <= 0;
      reload_value <= 0;
    end else begin
      // AXI register writes
      if (axi_write_en) begin
        if (axi_addr == 1) begin
          match_value <= axi_wdata;
        end else if (axi_addr == 2) begin
          reload_value <= axi_wdata;
        end else if (axi_addr == 3) begin
          enable <= axi_wdata[0:0];
          interval_mode <= axi_wdata[1:1];
          interrupt_enable <= axi_wdata[2:2];
        end
      end
      // Counter logic
      if (enable) begin
        if (count == match_value) begin
          if (interval_mode) begin
            count <= reload_value;
          end
          if (~status_clear) begin
            match_flag <= 1'b1;
          end else begin
            match_flag <= 1'b0;
          end
        end else begin
          count <= 32'(count + 1);
        end
      end
      // match_flag clear on status write (when counter not at match)
      if (status_clear & ~(enable & (count == match_value))) begin
        match_flag <= 1'b0;
      end
      // Interrupt generation
      if (status_clear) begin
        interrupt <= 1'b0;
      end else if (match_flag & interrupt_enable) begin
        interrupt <= 1'b1;
      end
    end
  end
  // AXI read logic (combinational)
  always_comb begin
    if (axi_addr == 0) begin
      axi_rdata = count;
    end else if (axi_addr == 1) begin
      axi_rdata = match_value;
    end else if (axi_addr == 2) begin
      axi_rdata = reload_value;
    end else if (axi_addr == 3) begin
      axi_rdata = {29'd0, interrupt_enable, interval_mode, enable};
    end else if (axi_addr == 4) begin
      axi_rdata = 32'($unsigned(interrupt));
    end else begin
      axi_rdata = 0;
    end
  end

endmodule

