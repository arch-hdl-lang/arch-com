module thermostat (
  input logic i_clk,
  input logic i_rst,
  input logic [5:0] i_temp_feedback,
  input logic i_fan_on,
  input logic i_enable,
  input logic i_fault,
  input logic i_clr,
  output logic o_heater_full,
  output logic o_heater_medium,
  output logic o_heater_low,
  output logic o_aircon_full,
  output logic o_aircon_medium,
  output logic o_aircon_low,
  output logic o_fan,
  output logic [2:0] o_state
);

  logic fault_latch;
  logic [2:0] next_state;
  logic any_active;
  always_comb begin
    if (i_temp_feedback[5]) begin
      next_state = 3'd2;
    end else if (i_temp_feedback[4]) begin
      next_state = 3'd1;
    end else if (i_temp_feedback[3]) begin
      next_state = 3'd0;
    end else if (i_temp_feedback[0]) begin
      next_state = 3'd6;
    end else if (i_temp_feedback[1]) begin
      next_state = 3'd5;
    end else if (i_temp_feedback[2]) begin
      next_state = 3'd4;
    end else begin
      next_state = 3'd3;
    end
    any_active = next_state != 3'd3;
  end
  always_ff @(posedge i_clk or negedge i_rst) begin
    if ((!i_rst)) begin
      fault_latch <= 1'b0;
      o_aircon_full <= 1'b0;
      o_aircon_low <= 1'b0;
      o_aircon_medium <= 1'b0;
      o_fan <= 1'b0;
      o_heater_full <= 1'b0;
      o_heater_low <= 1'b0;
      o_heater_medium <= 1'b0;
      o_state <= 3;
    end else begin
      if (i_clr) begin
        fault_latch <= 1'b0;
      end else if (i_fault) begin
        fault_latch <= 1'b1;
      end
      if (i_clr) begin
        o_state <= 3'd3;
      end else if (~i_enable) begin
        o_state <= 3'd3;
      end else begin
        o_state <= next_state;
      end
      if (fault_latch | i_fault | ~i_enable) begin
        o_heater_full <= 1'b0;
        o_heater_medium <= 1'b0;
        o_heater_low <= 1'b0;
        o_aircon_full <= 1'b0;
        o_aircon_medium <= 1'b0;
        o_aircon_low <= 1'b0;
        o_fan <= 1'b0;
      end else begin
        o_heater_full <= next_state == 3'd2;
        o_heater_medium <= next_state == 3'd1;
        o_heater_low <= next_state == 3'd0;
        o_aircon_full <= next_state == 3'd6;
        o_aircon_medium <= next_state == 3'd5;
        o_aircon_low <= next_state == 3'd4;
        o_fan <= any_active | i_fan_on;
      end
    end
  end

endmodule

