module serial_line_code_converter #(
  parameter int CLK_DIV = 16
) (
  input logic clk,
  input logic reset_n,
  input logic serial_in,
  input logic [2:0] mode,
  output logic serial_out
);

  logic [3:0] clk_counter;
  logic clk_pulse;
  logic prev_value;
  logic prev_serial_in;
  logic nrz_out;
  logic rz_out;
  logic diff_out;
  logic inv_nrz_out;
  logic alt_invert_out;
  logic alt_invert_state;
  logic parity_out;
  logic scrambled_out;
  logic edge_triggered_out;
  // Clock pulse generation
  always_ff @(posedge clk or negedge reset_n) begin
    if ((!reset_n)) begin
      clk_counter <= 0;
      clk_pulse <= 0;
    end else begin
      if (clk_counter == 4'(CLK_DIV - 1)) begin
        clk_counter <= 4'd0;
        clk_pulse <= 1'd1;
      end else begin
        clk_counter <= 4'(clk_counter + 4'd1);
        clk_pulse <= 1'd0;
      end
    end
  end
  // Previous serial input tracking
  always_ff @(posedge clk or negedge reset_n) begin
    if ((!reset_n)) begin
      prev_serial_in <= 0;
      prev_value <= 0;
    end else begin
      prev_value <= serial_in;
      prev_serial_in <= prev_value;
    end
  end
  // NRZ pass-through
  always_ff @(posedge clk or negedge reset_n) begin
    if ((!reset_n)) begin
      nrz_out <= 0;
    end else begin
      nrz_out <= serial_in;
    end
  end
  // RZ encoding
  always_ff @(posedge clk or negedge reset_n) begin
    if ((!reset_n)) begin
      rz_out <= 0;
    end else begin
      if ((serial_in == 1'd1) & &(clk_pulse == 1'd1)) begin
        rz_out <= 1'd1;
      end else begin
        rz_out <= 1'd0;
      end
    end
  end
  // Differential encoding
  always_ff @(posedge clk or negedge reset_n) begin
    if ((!reset_n)) begin
      diff_out <= 0;
    end else begin
      diff_out <= serial_in ^ prev_value;
    end
  end
  // Inverted NRZ
  always_ff @(posedge clk or negedge reset_n) begin
    if ((!reset_n)) begin
      inv_nrz_out <= 0;
    end else begin
      inv_nrz_out <= ~serial_in;
    end
  end
  // NRZ with alternating bit inversion
  always_ff @(posedge clk or negedge reset_n) begin
    if ((!reset_n)) begin
      alt_invert_out <= 0;
      alt_invert_state <= 0;
    end else begin
      alt_invert_state <= ~alt_invert_state;
      if (alt_invert_state == 1'd1) begin
        alt_invert_out <= ~serial_in;
      end else begin
        alt_invert_out <= serial_in;
      end
    end
  end
  // Parity bit output (odd parity)
  always_ff @(posedge clk or negedge reset_n) begin
    if ((!reset_n)) begin
      parity_out <= 0;
    end else begin
      parity_out <= parity_out ^ serial_in;
    end
  end
  // Scrambled NRZ
  always_ff @(posedge clk or negedge reset_n) begin
    if ((!reset_n)) begin
      scrambled_out <= 0;
    end else begin
      scrambled_out <= serial_in ^ clk_counter[0:0];
    end
  end
  // Edge-triggered NRZ
  always_ff @(posedge clk or negedge reset_n) begin
    if ((!reset_n)) begin
      edge_triggered_out <= 0;
    end else begin
      edge_triggered_out <= serial_in & ~prev_serial_in;
    end
  end
  // Output multiplexer
  always_comb begin
    if (mode == 3'd0) begin
      serial_out = nrz_out;
    end else if (mode == 3'd1) begin
      serial_out = rz_out;
    end else if (mode == 3'd2) begin
      serial_out = diff_out;
    end else if (mode == 3'd3) begin
      serial_out = inv_nrz_out;
    end else if (mode == 3'd4) begin
      serial_out = alt_invert_out;
    end else if (mode == 3'd5) begin
      serial_out = parity_out;
    end else if (mode == 3'd6) begin
      serial_out = scrambled_out;
    end else if (mode == 3'd7) begin
      serial_out = edge_triggered_out;
    end else begin
      serial_out = 1'd0;
    end
  end

endmodule

