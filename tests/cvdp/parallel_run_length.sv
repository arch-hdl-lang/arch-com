module parallel_run_length #(
  parameter int DATA_WIDTH = 8,
  parameter int NUM_STREAMS = 4,
  localparam int RV_W = $clog2(DATA_WIDTH) + 1,
  localparam int DW_VAL = DATA_WIDTH
) (
  input logic clk,
  input logic reset_n,
  input logic [NUM_STREAMS-1:0] data_in,
  input logic [NUM_STREAMS-1:0] stream_enable,
  output logic [NUM_STREAMS-1:0] data_out,
  output logic [NUM_STREAMS * RV_W-1:0] run_value,
  output logic [NUM_STREAMS-1:0] valid
);

  logic prev_data [NUM_STREAMS-1:0];
  logic [RV_W-1:0] run_cnt [NUM_STREAMS-1:0];
  logic [NUM_STREAMS-1:0] valid_r;
  logic [NUM_STREAMS-1:0] data_out_r;
  logic [RV_W-1:0] run_val_r [NUM_STREAMS-1:0];
  always_ff @(posedge clk or negedge reset_n) begin
    if ((!reset_n)) begin
      data_out_r <= 0;
      for (int __ri0 = 0; __ri0 < NUM_STREAMS; __ri0++) begin
        prev_data[__ri0] <= 0;
      end
      for (int __ri0 = 0; __ri0 < NUM_STREAMS; __ri0++) begin
        run_cnt[__ri0] <= 0;
      end
      for (int __ri0 = 0; __ri0 < NUM_STREAMS; __ri0++) begin
        run_val_r[__ri0] <= 0;
      end
      valid_r <= 0;
    end else begin
      for (int i = 0; i <= NUM_STREAMS - 1; i++) begin
        if (~stream_enable[i +: 1]) begin
          // Disabled: reset everything
          prev_data[i] <= 0;
          run_cnt[i] <= 0;
          valid_r[i] <= 0;
          data_out_r[i] <= 0;
          run_val_r[i] <= 0;
        end else begin
          // Always update prev_data when stream is enabled
          prev_data[i] <= data_in[i +: 1];
          if (data_in[i +: 1] != prev_data[i]) begin
            // Data changed: output the old run
            valid_r[i] <= 1;
            data_out_r[i] <= prev_data[i];
            run_val_r[i] <= run_cnt[i];
            run_cnt[i] <= RV_W'($unsigned(1));
          end else if (run_cnt[i] == DW_VAL[RV_W - 1:0]) begin
            // Max run length reached: output and reset
            valid_r[i] <= 1;
            data_out_r[i] <= prev_data[i];
            run_val_r[i] <= run_cnt[i];
            run_cnt[i] <= RV_W'($unsigned(1));
          end else begin
            // Continue run
            valid_r[i] <= 0;
            data_out_r[i] <= 0;
            run_val_r[i] <= 0;
            run_cnt[i] <= RV_W'(run_cnt[i] + 1);
          end
        end
      end
    end
  end
  // Pack run_val_r into run_value output
  logic [NUM_STREAMS * RV_W-1:0] pack_acc [NUM_STREAMS + 1-1:0];
  always_comb begin
    pack_acc[0] = 0;
    for (int i = 0; i <= NUM_STREAMS - 1; i++) begin
      pack_acc[i + 1] = pack_acc[i] | (NUM_STREAMS * RV_W)'($unsigned(run_val_r[i])) << i * RV_W;
    end
    run_value = pack_acc[NUM_STREAMS];
  end
  assign valid = valid_r;
  assign data_out = data_out_r;

endmodule

