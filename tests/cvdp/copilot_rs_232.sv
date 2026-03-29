module copilot_rs_232 #(
  parameter int CLOCK_FREQ = 100000000,
  parameter int BAUD_RATE = 115200,
  parameter int BAUD_ACC_WIDTH = 16,
  parameter int REG_INPUT = 1
) (
  input logic clock,
  input logic reset_neg,
  input logic tx_datain_ready,
  input logic Present_Processing_Completed,
  input logic [8-1:0] tx_datain,
  output logic tx_transmitter,
  output logic tx_transmitter_valid
);

  logic [4-1:0] State;
  logic [8-1:0] tx_data_reg;
  logic baud_pulse_w;
  logic MuxBit;
  baud_rate_generator #(.BAUD_ACC_WIDTH(BAUD_ACC_WIDTH), .CLOCK_FREQ(CLOCK_FREQ), .BAUD_RATE(BAUD_RATE)) baud_gen (
    .clock(clock),
    .reset_neg(reset_neg),
    .enable(tx_transmitter_valid),
    .baud_pulse(baud_pulse_w)
  );
  assign tx_transmitter_valid = State != 0;
  always_ff @(posedge clock or negedge reset_neg) begin
    if ((!reset_neg)) begin
      tx_data_reg <= 0;
    end else begin
      if (~reset_neg) begin
        tx_data_reg <= 0;
      end else if (Present_Processing_Completed) begin
        tx_data_reg <= 0;
      end else if (tx_datain_ready & State == 0) begin
        tx_data_reg <= tx_datain;
      end
    end
  end
  always_ff @(posedge clock or negedge reset_neg) begin
    if ((!reset_neg)) begin
      State <= 0;
    end else begin
      if (~reset_neg) begin
        State <= 0;
      end else if (Present_Processing_Completed) begin
        State <= 0;
      end else if (State == 0 & tx_datain_ready) begin
        State <= 4;
      end else if (baud_pulse_w) begin
        if (State == 15) begin
          State <= 1;
        end else if (State == 1) begin
          State <= 0;
        end else begin
          State <= 4'(State + 1);
        end
      end
    end
  end
  // Output mux
  always_comb begin
    if (State[2:0] == 0) begin
      MuxBit = tx_data_reg[0];
    end else if (State[2:0] == 1) begin
      MuxBit = tx_data_reg[1];
    end else if (State[2:0] == 2) begin
      MuxBit = tx_data_reg[2];
    end else if (State[2:0] == 3) begin
      MuxBit = tx_data_reg[3];
    end else if (State[2:0] == 4) begin
      MuxBit = tx_data_reg[4];
    end else if (State[2:0] == 5) begin
      MuxBit = tx_data_reg[5];
    end else if (State[2:0] == 6) begin
      MuxBit = tx_data_reg[6];
    end else begin
      MuxBit = tx_data_reg[7];
    end
  end
  logic tx_out;
  always_ff @(posedge clock or negedge reset_neg) begin
    if ((!reset_neg)) begin
      tx_out <= 1;
    end else begin
      if (~reset_neg) begin
        tx_out <= 1'b1;
      end else if (Present_Processing_Completed) begin
        tx_out <= 1'b1;
      end else begin
        tx_out <= State < 4 | State[3] & MuxBit;
      end
    end
  end
  assign tx_transmitter = tx_out;

endmodule

module baud_rate_generator #(
  parameter int CLOCK_FREQ = 100000000,
  parameter int BAUD_RATE = 115200,
  parameter int BAUD_ACC_WIDTH = 16
) (
  input logic clock,
  input logic reset_neg,
  input logic enable,
  output logic baud_pulse
);

  logic [BAUD_ACC_WIDTH + 1-1:0] baud_inc;
  assign baud_inc = (BAUD_ACC_WIDTH + 1)'(((BAUD_RATE << BAUD_ACC_WIDTH - 4) + (CLOCK_FREQ >> 5)) / (CLOCK_FREQ >> 4));
  logic [BAUD_ACC_WIDTH + 1-1:0] baud_acc;
  always_ff @(posedge clock or negedge reset_neg) begin
    if ((!reset_neg)) begin
      baud_acc <= 0;
    end else begin
      if (~reset_neg) begin
        baud_acc <= 0;
      end else if (enable) begin
        baud_acc <= (BAUD_ACC_WIDTH + 1)'((BAUD_ACC_WIDTH + 1)'($unsigned(baud_acc[BAUD_ACC_WIDTH - 1:0])) + baud_inc);
      end else begin
        baud_acc <= 0;
      end
    end
  end
  assign baud_pulse = baud_acc[BAUD_ACC_WIDTH];

endmodule

