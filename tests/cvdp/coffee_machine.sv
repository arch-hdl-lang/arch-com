module coffee_machine #(
  parameter int NBW_DLY = 5,
  parameter int NBW_BEANS = 2,
  parameter int NS_BEANS = 4,
  localparam int BEAN_SEL_DELAY = 3,
  localparam int POWDER_DELAY = 2
) (
  input logic clk,
  input logic rst_async_n,
  input logic [2:0] i_operation_sel,
  input logic i_start,
  input logic [3:0] i_sensor,
  input logic [NBW_DLY-1:0] i_grind_delay,
  input logic [NBW_DLY-1:0] i_heat_delay,
  input logic [NBW_DLY-1:0] i_pour_delay,
  input logic [NBW_BEANS-1:0] i_bean_sel,
  output logic [NS_BEANS-1:0] o_bean_sel,
  output logic o_grind_beans,
  output logic o_use_powder,
  output logic o_heat_water,
  output logic o_pour_coffee,
  output logic o_error
);

  // Combinational outputs — reflect current state same cycle (no port reg lag)
  // State register: 0=IDLE, 1=BEAN_SEL, 2=GRIND, 3=HEAT, 4=POWDER, 5=POUR
  logic [2:0] state_ff;
  // Registered i_start: delays transition by one cycle so outputs align with model
  logic i_start_r;
  // latched bean index (for one-hot generation)
  logic [NS_BEANS-1:0] bean_r;
  logic [2:0] op_r;
  logic [NBW_DLY-1:0] cnt_r;
  logic [NBW_DLY-1:0] grind_dly_r;
  logic [NBW_DLY-1:0] heat_dly_r;
  logic [NBW_DLY-1:0] pour_dly_r;
  // Sensor error decode (combinational)
  logic generic_err;
  assign generic_err = i_sensor[3];
  logic no_water_err;
  assign no_water_err = i_sensor[0];
  logic no_beans_err;
  assign no_beans_err = i_sensor[1] & (i_operation_sel == 2 | i_operation_sel == 3);
  logic no_powder_err;
  assign no_powder_err = i_sensor[2] & (i_operation_sel == 1 | i_operation_sel == 4);
  logic bad_op_err;
  assign bad_op_err = i_operation_sel == 6 | i_operation_sel == 7;
  // o_error: combinational
  logic o_error_w;
  always_comb begin
    if (state_ff == 0) begin
      // IDLE
      o_error_w = generic_err | no_water_err | no_beans_err | no_powder_err | bad_op_err;
    end else if (generic_err) begin
      o_error_w = 1'b1;
    end else begin
      o_error_w = 1'b0;
    end
  end
  assign o_error = o_error_w;
  // Combinational outputs based on current state_ff (0-cycle, no lag)
  always_comb begin
    if (state_ff == 0) begin
      // IDLE
      o_bean_sel = 0;
      o_grind_beans = 1'b0;
      o_use_powder = 1'b0;
      o_heat_water = 1'b0;
      o_pour_coffee = 1'b0;
    end else if (state_ff == 1) begin
      // BEAN_SEL
      o_bean_sel = bean_r;
      o_grind_beans = 1'b0;
      o_use_powder = 1'b0;
      o_heat_water = 1'b0;
      o_pour_coffee = 1'b0;
    end else if (state_ff == 2) begin
      // GRIND
      o_bean_sel = bean_r;
      o_grind_beans = 1'b1;
      o_use_powder = 1'b0;
      o_heat_water = 1'b0;
      o_pour_coffee = 1'b0;
    end else if (state_ff == 3) begin
      // HEAT
      o_bean_sel = 0;
      o_grind_beans = 1'b0;
      o_use_powder = 1'b0;
      o_heat_water = 1'b1;
      o_pour_coffee = 1'b0;
    end else if (state_ff == 4) begin
      // POWDER
      o_bean_sel = 0;
      o_grind_beans = 1'b0;
      o_use_powder = 1'b1;
      o_heat_water = 1'b0;
      o_pour_coffee = 1'b0;
    end else if (state_ff == 5) begin
      // POUR
      o_bean_sel = 0;
      o_grind_beans = 1'b0;
      o_use_powder = 1'b0;
      o_heat_water = 1'b0;
      o_pour_coffee = 1'b1;
    end else begin
      o_bean_sel = 0;
      o_grind_beans = 1'b0;
      o_use_powder = 1'b0;
      o_heat_water = 1'b0;
      o_pour_coffee = 1'b0;
    end
  end
  always_ff @(posedge clk or negedge rst_async_n) begin
    if ((!rst_async_n)) begin
      bean_r <= 0;
      cnt_r <= 0;
      grind_dly_r <= 0;
      heat_dly_r <= 0;
      i_start_r <= 1'b0;
      op_r <= 0;
      pour_dly_r <= 0;
      state_ff <= 0;
    end else begin
      // Register i_start (delays IDLE->state transition by 1 cycle for correct output timing)
      i_start_r <= i_start;
      // State machine transitions
      if (state_ff == 0) begin
        // IDLE
        if (i_start_r & ~o_error_w) begin
          op_r <= i_operation_sel;
          bean_r <= NS_BEANS'($unsigned(1)) << i_bean_sel;
          grind_dly_r <= i_grind_delay;
          heat_dly_r <= i_heat_delay;
          pour_dly_r <= i_pour_delay;
          cnt_r <= 0;
          if (i_operation_sel == 0) begin
            state_ff <= 3;
          end else if (i_operation_sel == 1) begin
            // HEAT
            state_ff <= 3;
          end else if (i_operation_sel == 2) begin
            // HEAT
            state_ff <= 1;
          end else if (i_operation_sel == 3) begin
            // BEAN_SEL
            state_ff <= 1;
          end else if (i_operation_sel == 4) begin
            // BEAN_SEL
            state_ff <= 4;
          end else if (i_operation_sel == 5) begin
            // POWDER
            state_ff <= 5;
          end
        end
      end else if (state_ff == 1) begin
        // POUR
        // BEAN_SEL
        if (generic_err) begin
          state_ff <= 0;
          cnt_r <= 0;
        end else if (32'($unsigned(cnt_r)) < BEAN_SEL_DELAY - 1) begin
          cnt_r <= NBW_DLY'(cnt_r + 1);
        end else begin
          cnt_r <= 0;
          state_ff <= 2;
        end
      end else if (state_ff == 2) begin
        // -> GRIND
        // GRIND
        if (generic_err) begin
          state_ff <= 0;
          cnt_r <= 0;
        end else if (cnt_r < grind_dly_r - 1) begin
          cnt_r <= NBW_DLY'(cnt_r + 1);
        end else begin
          cnt_r <= 0;
          if (op_r == 2) begin
            state_ff <= 3;
          end else begin
            // -> HEAT
            // op_r == 3
            state_ff <= 4;
          end
        end
      end else if (state_ff == 3) begin
        // -> POWDER
        // HEAT
        if (generic_err) begin
          state_ff <= 0;
          cnt_r <= 0;
        end else if (cnt_r < heat_dly_r - 1) begin
          cnt_r <= NBW_DLY'(cnt_r + 1);
        end else begin
          cnt_r <= 0;
          if (op_r == 0) begin
            state_ff <= 5;
          end else begin
            // -> POUR
            // op_r in {1,2}: heat -> powder
            state_ff <= 4;
          end
        end
      end else if (state_ff == 4) begin
        // POWDER
        if (generic_err) begin
          state_ff <= 0;
          cnt_r <= 0;
        end else if (32'($unsigned(cnt_r)) < POWDER_DELAY - 1) begin
          cnt_r <= NBW_DLY'(cnt_r + 1);
        end else begin
          cnt_r <= 0;
          state_ff <= 5;
        end
      end else if (state_ff == 5) begin
        // -> POUR
        // POUR
        if (generic_err) begin
          state_ff <= 0;
          cnt_r <= 0;
        end else if (cnt_r < pour_dly_r - 1) begin
          cnt_r <= NBW_DLY'(cnt_r + 1);
        end else begin
          cnt_r <= 0;
          state_ff <= 0;
        end
      end
      // -> IDLE
    end
  end

endmodule

