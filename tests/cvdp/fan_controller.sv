module fan_controller (
  input logic clk,
  input logic reset,
  input logic psel,
  input logic penable,
  input logic pwrite,
  input logic [8-1:0] paddr,
  input logic [8-1:0] pwdata,
  output logic fan_pwm_out,
  output logic [8-1:0] prdata,
  output logic pready,
  output logic pslverr
);

  // Internal registers for temperature thresholds and ADC reading
  logic [8-1:0] temp_low;
  logic [8-1:0] temp_med;
  logic [8-1:0] temp_high;
  logic [8-1:0] temp_adc_in;
  // PWM registers
  logic [8-1:0] pwm_duty_cycle;
  logic [8-1:0] pwm_counter;
  // APB helper signals
  logic apb_access;
  assign apb_access = psel & penable;
  // APB read/write logic
  logic [8-1:0] w_prdata;
  logic w_pslverr;
  always_comb begin
    w_prdata = 0;
    w_pslverr = 1'b0;
    if (apb_access) begin
      if (paddr == 10) begin
        w_prdata = temp_low;
      end else if (paddr == 11) begin
        w_prdata = temp_med;
      end else if (paddr == 12) begin
        w_prdata = temp_high;
      end else if (paddr == 15) begin
        w_prdata = temp_adc_in;
      end else begin
        w_pslverr = 1'b1;
      end
    end
  end
  // APB write
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      temp_adc_in <= 0;
      temp_high <= 0;
      temp_low <= 0;
      temp_med <= 0;
    end else begin
      if (apb_access & pwrite) begin
        if (paddr == 10) begin
          temp_low <= pwdata;
        end else if (paddr == 11) begin
          temp_med <= pwdata;
        end else if (paddr == 12) begin
          temp_high <= pwdata;
        end else if (paddr == 15) begin
          temp_adc_in <= pwdata;
        end
      end
    end
  end
  // Duty cycle computation based on temperature
  logic [8-1:0] w_duty;
  always_comb begin
    if (temp_adc_in < temp_low) begin
      w_duty = 64;
    end else if (temp_adc_in < temp_med) begin
      w_duty = 128;
    end else if (temp_adc_in < temp_high) begin
      w_duty = 192;
    end else begin
      w_duty = 255;
    end
  end
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      pwm_duty_cycle <= 0;
    end else begin
      pwm_duty_cycle <= w_duty;
    end
  end
  // PWM counter
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      pwm_counter <= 0;
    end else begin
      if (pwm_counter == 255) begin
        pwm_counter <= 0;
      end else begin
        pwm_counter <= 8'(pwm_counter + 1);
      end
    end
  end
  // PWM output: high when counter in [1, pwm_duty_cycle]
  logic cnt_nonzero;
  assign cnt_nonzero = pwm_counter != 0;
  logic cnt_in_duty;
  assign cnt_in_duty = pwm_duty_cycle > pwm_counter | pwm_duty_cycle == pwm_counter;
  // Output assignments
  assign fan_pwm_out = cnt_nonzero & cnt_in_duty;
  assign prdata = w_prdata;
  assign pready = apb_access;
  assign pslverr = w_pslverr;

endmodule

