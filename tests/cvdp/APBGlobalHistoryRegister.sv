module ApbGhrIcg (
  input logic clk_in,
  input logic enable,
  input logic test_en,
  output logic clk_out
);

  logic en_latched;
  always_latch if (!clk_in) en_latched = enable | test_en;
  assign clk_out = clk_in & en_latched;

endmodule

module APBGlobalHistoryRegister (
  input logic pclk,
  input logic presetn,
  input logic [10-1:0] paddr,
  input logic pselx,
  input logic penable,
  input logic pwrite,
  input logic [8-1:0] pwdata,
  output logic pready,
  output logic [8-1:0] prdata,
  output logic pslverr,
  input logic history_shift_valid,
  input logic clk_gate_en,
  output logic history_full,
  output logic history_empty,
  output logic error_flag,
  output logic interrupt_full,
  output logic interrupt_error
);

  // Clock gate: enable is active-low (clk_gate_en=0 means clock active)
  logic pclk_gated;
  ApbGhrIcg icg (
    .clk_in(pclk),
    .enable(~clk_gate_en),
    .test_en(1'b0),
    .clk_out(pclk_gated)
  );
  // CSR registers
  logic [8-1:0] control_reg;
  logic [8-1:0] train_hist;
  logic [8-1:0] predict_hist;
  // APB interface on gated clock
  always_ff @(posedge pclk_gated or negedge presetn) begin
    if ((!presetn)) begin
      control_reg <= 0;
      prdata <= 0;
      pready <= 1'b0;
      pslverr <= 1'b0;
      train_hist <= 0;
    end else begin
      pready <= 1'b1;
      if (pselx & penable) begin
        if (paddr == 0) begin
          if (pwrite) begin
            control_reg <= pwdata;
          end
          prdata <= control_reg;
          pslverr <= 1'b0;
        end else if (paddr == 1) begin
          if (pwrite) begin
            train_hist <= {1'd0, pwdata[6:0]};
          end
          prdata <= train_hist;
          pslverr <= 1'b0;
        end else if (paddr == 2) begin
          prdata <= predict_hist;
          pslverr <= 1'b0;
        end else begin
          pslverr <= 1'b1;
          prdata <= 0;
        end
      end
    end
  end
  // Decode control register fields
  logic ctrl_predict_valid;
  assign ctrl_predict_valid = control_reg[0:0] == 1;
  logic ctrl_predict_taken;
  assign ctrl_predict_taken = control_reg[1:1] == 1;
  logic ctrl_train_mispredicted;
  assign ctrl_train_mispredicted = control_reg[2:2] == 1;
  logic ctrl_train_taken;
  assign ctrl_train_taken = control_reg[3:3] == 1;
  // Predict history update on rising edge of history_shift_valid
  always_ff @(posedge history_shift_valid or negedge presetn) begin
    if ((!presetn)) begin
      predict_hist <= 0;
    end else begin
      if (ctrl_train_mispredicted) begin
        predict_hist <= {train_hist[6:0], ctrl_train_taken};
      end else if (ctrl_predict_valid) begin
        predict_hist <= {predict_hist[6:0], ctrl_predict_taken};
      end
    end
  end
  // Status outputs
  assign history_full = predict_hist == 8'd255;
  assign history_empty = predict_hist == 0;
  assign error_flag = pslverr;
  assign interrupt_full = predict_hist == 8'd255;
  assign interrupt_error = pslverr;

endmodule

