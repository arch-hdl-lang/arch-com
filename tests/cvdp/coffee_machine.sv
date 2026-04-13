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

  // State register: 0=IDLE, 1=BEAN_SEL, 2=GRIND, 3=HEAT, 4=POWDER, 5=POUR
  logic [2:0] state_ff;
  logic [2:0] op_r;
  logic [NBW_DLY-1:0] cnt_r;
  logic [NBW_BEANS-1:0] bean_r;
  logic [NBW_DLY-1:0] grind_dly_r;
  logic [NBW_DLY-1:0] heat_dly_r;
  logic [NBW_DLY-1:0] pour_dly_r;
  // Combinational error/sensor decode
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
  // One-hot bean select from stored bean index
  logic [NS_BEANS-1:0] bean_onehot;
  assign bean_onehot = bean_r == 0 ? 1 : bean_r == 1 ? 2 : bean_r == 2 ? 4 : 8;
  // Registered outputs are computed from the CURRENT state_ff (one-cycle lag)
  // Plus separate state transition logic
  always_ff @(posedge clk or negedge rst_async_n) begin
    if ((!rst_async_n)) begin
      bean_r <= 0;
      cnt_r <= 0;
      grind_dly_r <= 0;
      heat_dly_r <= 0;
      o_bean_sel <= 0;
      o_grind_beans <= 1'b0;
      o_heat_water <= 1'b0;
      o_pour_coffee <= 1'b0;
      o_use_powder <= 1'b0;
      op_r <= 0;
      pour_dly_r <= 0;
      state_ff <= 0;
    end else begin
      // Step 1: Set outputs based on CURRENT state (these become the NEXT cycle's visible outputs)
      if (state_ff == 0) begin
        // IDLE: all outputs = 0
        o_bean_sel <= 0;
        o_grind_beans <= 1'b0;
        o_use_powder <= 1'b0;
        o_heat_water <= 1'b0;
        o_pour_coffee <= 1'b0;
      end else if (state_ff == 1) begin
        // BEAN_SEL
        o_bean_sel <= bean_onehot;
        o_grind_beans <= 1'b0;
        o_use_powder <= 1'b0;
        o_heat_water <= 1'b0;
        o_pour_coffee <= 1'b0;
      end else if (state_ff == 2) begin
        // GRIND
        o_bean_sel <= bean_onehot;
        o_grind_beans <= 1'b1;
        o_use_powder <= 1'b0;
        o_heat_water <= 1'b0;
        o_pour_coffee <= 1'b0;
      end else if (state_ff == 3) begin
        // HEAT
        o_bean_sel <= 0;
        o_grind_beans <= 1'b0;
        o_use_powder <= 1'b0;
        o_heat_water <= 1'b1;
        o_pour_coffee <= 1'b0;
      end else if (state_ff == 4) begin
        // POWDER
        o_bean_sel <= 0;
        o_grind_beans <= 1'b0;
        o_use_powder <= 1'b1;
        o_heat_water <= 1'b0;
        o_pour_coffee <= 1'b0;
      end else if (state_ff == 5) begin
        // POUR
        o_bean_sel <= 0;
        o_grind_beans <= 1'b0;
        o_use_powder <= 1'b0;
        o_heat_water <= 1'b0;
        o_pour_coffee <= 1'b1;
      end
      // Step 2: State transitions (update state_ff for next cycle)
      if (state_ff == 0) begin
        // IDLE
        if (i_start & ~o_error_w) begin
          op_r <= i_operation_sel;
          bean_r <= i_bean_sel;
          grind_dly_r <= i_grind_delay;
          heat_dly_r <= i_heat_delay;
          pour_dly_r <= i_pour_delay;
          cnt_r <= 0;
          if (i_operation_sel == 0) begin
            // heat -> pour
            state_ff <= 3;
          end else if (i_operation_sel == 1) begin
            // heat -> powder -> pour
            state_ff <= 3;
          end else if (i_operation_sel == 2) begin
            // bean_sel -> grind -> heat -> powder -> pour
            state_ff <= 1;
          end else if (i_operation_sel == 3) begin
            // bean_sel -> grind -> powder -> pour
            state_ff <= 1;
          end else if (i_operation_sel == 4) begin
            // powder -> pour
            state_ff <= 4;
          end else if (i_operation_sel == 5) begin
            // pour
            state_ff <= 5;
          end
        end
      end else if (state_ff == 1) begin
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
            // heat -> pour
            state_ff <= 5;
          end else begin
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

