module interrupt_controller #(
  parameter int STARVATION_THRESHOLD = 5
) (
  input logic clk,
  input logic rst_n,
  input logic reset_interrupts,
  input logic [9:0] interrupt_requests,
  input logic interrupt_ack,
  input logic interrupt_trig,
  input logic [9:0] interrupt_mask,
  input logic [3:0] priority_override,
  input logic [3:0] override_interrupt_id,
  input logic priority_override_en,
  output logic [3:0] interrupt_id,
  output logic interrupt_valid,
  output logic [9:0] interrupt_status,
  output logic [9:0] missed_interrupts,
  output logic starvation_detected
);

  // FSM states
  // 0=IDLE, 1=PRIORITY_CALC, 2=SERVICE_PREP, 3=SERVICING, 4=COMPLETION, 7=ERROR
  logic [2:0] current_state;
  logic [9:0] pending_interrupts;
  logic [9:0] r_interrupt_status;
  logic [9:0] r_missed_interrupts;
  logic [3:0] r_interrupt_id;
  logic r_interrupt_valid;
  logic r_starvation_detected;
  // Wait counters for each interrupt (4 bits each)
  logic [3:0] wait_cnt_0;
  logic [3:0] wait_cnt_1;
  logic [3:0] wait_cnt_2;
  logic [3:0] wait_cnt_3;
  logic [3:0] wait_cnt_4;
  logic [3:0] wait_cnt_5;
  logic [3:0] wait_cnt_6;
  logic [3:0] wait_cnt_7;
  logic [3:0] wait_cnt_8;
  logic [3:0] wait_cnt_9;
  // Effective priority for each interrupt (5 bits each)
  logic [4:0] eff_pri_0;
  logic [4:0] eff_pri_1;
  logic [4:0] eff_pri_2;
  logic [4:0] eff_pri_3;
  logic [4:0] eff_pri_4;
  logic [4:0] eff_pri_5;
  logic [4:0] eff_pri_6;
  logic [4:0] eff_pri_7;
  logic [4:0] eff_pri_8;
  logic [4:0] eff_pri_9;
  logic [3:0] service_timer;
  logic timeout_error;
  logic [3:0] next_interrupt_id;
  logic [4:0] max_priority;
  // Active mask
  logic [9:0] active_mask;
  // Wires for combinational priority calculation
  logic [4:0] w_eff_pri_0;
  logic [4:0] w_eff_pri_1;
  logic [4:0] w_eff_pri_2;
  logic [4:0] w_eff_pri_3;
  logic [4:0] w_eff_pri_4;
  logic [4:0] w_eff_pri_5;
  logic [4:0] w_eff_pri_6;
  logic [4:0] w_eff_pri_7;
  logic [4:0] w_eff_pri_8;
  logic [4:0] w_eff_pri_9;
  logic [4:0] w_max_pri;
  logic [3:0] w_max_id;
  logic [9:0] mask_inv;
  logic any_starvation;
  // Inverted mask
  assign mask_inv = ~interrupt_mask;
  // Compute effective priorities combinationally
  always_comb begin
    // Base priorities: (10-i), override if enabled
    if (priority_override_en & (override_interrupt_id == 0)) begin
      w_eff_pri_0 = 5'($unsigned(priority_override));
    end else begin
      w_eff_pri_0 = 10;
    end
    if (priority_override_en & (override_interrupt_id == 1)) begin
      w_eff_pri_1 = 5'($unsigned(priority_override));
    end else begin
      w_eff_pri_1 = 9;
    end
    if (priority_override_en & (override_interrupt_id == 2)) begin
      w_eff_pri_2 = 5'($unsigned(priority_override));
    end else begin
      w_eff_pri_2 = 8;
    end
    if (priority_override_en & (override_interrupt_id == 3)) begin
      w_eff_pri_3 = 5'($unsigned(priority_override));
    end else begin
      w_eff_pri_3 = 7;
    end
    if (priority_override_en & (override_interrupt_id == 4)) begin
      w_eff_pri_4 = 5'($unsigned(priority_override));
    end else begin
      w_eff_pri_4 = 6;
    end
    if (priority_override_en & (override_interrupt_id == 5)) begin
      w_eff_pri_5 = 5'($unsigned(priority_override));
    end else begin
      w_eff_pri_5 = 5;
    end
    if (priority_override_en & (override_interrupt_id == 6)) begin
      w_eff_pri_6 = 5'($unsigned(priority_override));
    end else begin
      w_eff_pri_6 = 4;
    end
    if (priority_override_en & (override_interrupt_id == 7)) begin
      w_eff_pri_7 = 5'($unsigned(priority_override));
    end else begin
      w_eff_pri_7 = 3;
    end
    if (priority_override_en & (override_interrupt_id == 8)) begin
      w_eff_pri_8 = 5'($unsigned(priority_override));
    end else begin
      w_eff_pri_8 = 2;
    end
    if (priority_override_en & (override_interrupt_id == 9)) begin
      w_eff_pri_9 = 5'($unsigned(priority_override));
    end else begin
      w_eff_pri_9 = 1;
    end
  end
  // Find max priority pending interrupt (>=, so highest index wins on tie)
  always_comb begin
    w_max_pri = 0;
    w_max_id = 0;
    if (pending_interrupts[0:0] == 1) begin
      if (eff_pri_0 >= w_max_pri) begin
        w_max_pri = eff_pri_0;
        w_max_id = 0;
      end
    end
    if (pending_interrupts[1:1] == 1) begin
      if (eff_pri_1 >= w_max_pri) begin
        w_max_pri = eff_pri_1;
        w_max_id = 1;
      end
    end
    if (pending_interrupts[2:2] == 1) begin
      if (eff_pri_2 >= w_max_pri) begin
        w_max_pri = eff_pri_2;
        w_max_id = 2;
      end
    end
    if (pending_interrupts[3:3] == 1) begin
      if (eff_pri_3 >= w_max_pri) begin
        w_max_pri = eff_pri_3;
        w_max_id = 3;
      end
    end
    if (pending_interrupts[4:4] == 1) begin
      if (eff_pri_4 >= w_max_pri) begin
        w_max_pri = eff_pri_4;
        w_max_id = 4;
      end
    end
    if (pending_interrupts[5:5] == 1) begin
      if (eff_pri_5 >= w_max_pri) begin
        w_max_pri = eff_pri_5;
        w_max_id = 5;
      end
    end
    if (pending_interrupts[6:6] == 1) begin
      if (eff_pri_6 >= w_max_pri) begin
        w_max_pri = eff_pri_6;
        w_max_id = 6;
      end
    end
    if (pending_interrupts[7:7] == 1) begin
      if (eff_pri_7 >= w_max_pri) begin
        w_max_pri = eff_pri_7;
        w_max_id = 7;
      end
    end
    if (pending_interrupts[8:8] == 1) begin
      if (eff_pri_8 >= w_max_pri) begin
        w_max_pri = eff_pri_8;
        w_max_id = 8;
      end
    end
    if (pending_interrupts[9:9] == 1) begin
      if (eff_pri_9 >= w_max_pri) begin
        w_max_pri = eff_pri_9;
        w_max_id = 9;
      end
    end
  end
  // Starvation detection
  assign any_starvation = (wait_cnt_0 >= STARVATION_THRESHOLD) | (wait_cnt_1 >= STARVATION_THRESHOLD) | (wait_cnt_2 >= STARVATION_THRESHOLD) | (wait_cnt_3 >= STARVATION_THRESHOLD) | (wait_cnt_4 >= STARVATION_THRESHOLD) | (wait_cnt_5 >= STARVATION_THRESHOLD) | (wait_cnt_6 >= STARVATION_THRESHOLD) | (wait_cnt_7 >= STARVATION_THRESHOLD) | (wait_cnt_8 >= STARVATION_THRESHOLD) | (wait_cnt_9 >= STARVATION_THRESHOLD);
  // Main FSM
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      active_mask <= 0;
      current_state <= 0;
      eff_pri_0 <= 0;
      eff_pri_1 <= 0;
      eff_pri_2 <= 0;
      eff_pri_3 <= 0;
      eff_pri_4 <= 0;
      eff_pri_5 <= 0;
      eff_pri_6 <= 0;
      eff_pri_7 <= 0;
      eff_pri_8 <= 0;
      eff_pri_9 <= 0;
      max_priority <= 0;
      next_interrupt_id <= 0;
      pending_interrupts <= 0;
      r_interrupt_id <= 0;
      r_interrupt_status <= 0;
      r_interrupt_valid <= 1'b0;
      r_missed_interrupts <= 0;
      r_starvation_detected <= 1'b0;
      service_timer <= 0;
      timeout_error <= 1'b0;
      wait_cnt_0 <= 0;
      wait_cnt_1 <= 0;
      wait_cnt_2 <= 0;
      wait_cnt_3 <= 0;
      wait_cnt_4 <= 0;
      wait_cnt_5 <= 0;
      wait_cnt_6 <= 0;
      wait_cnt_7 <= 0;
      wait_cnt_8 <= 0;
      wait_cnt_9 <= 0;
    end else begin
      if (reset_interrupts) begin
        current_state <= 0;
        pending_interrupts <= 0;
        r_interrupt_status <= 0;
        r_missed_interrupts <= 0;
        r_interrupt_valid <= 1'b0;
        r_interrupt_id <= 0;
        r_starvation_detected <= 1'b0;
        wait_cnt_0 <= 0;
        wait_cnt_1 <= 0;
        wait_cnt_2 <= 0;
        wait_cnt_3 <= 0;
        wait_cnt_4 <= 0;
        wait_cnt_5 <= 0;
        wait_cnt_6 <= 0;
        wait_cnt_7 <= 0;
        wait_cnt_8 <= 0;
        wait_cnt_9 <= 0;
        eff_pri_0 <= 0;
        eff_pri_1 <= 0;
        eff_pri_2 <= 0;
        eff_pri_3 <= 0;
        eff_pri_4 <= 0;
        eff_pri_5 <= 0;
        eff_pri_6 <= 0;
        eff_pri_7 <= 0;
        eff_pri_8 <= 0;
        eff_pri_9 <= 0;
        service_timer <= 0;
        timeout_error <= 1'b0;
        next_interrupt_id <= 0;
        max_priority <= 0;
        active_mask <= 0;
      end else begin
        // Capture new interrupts on trig
        if (interrupt_trig) begin
          pending_interrupts <= pending_interrupts | (interrupt_requests & mask_inv);
          // Track missed (masked) interrupts
          r_missed_interrupts <= r_missed_interrupts | (interrupt_requests & interrupt_mask);
        end
        // State machine
        if (current_state == 0) begin
          // IDLE
          r_interrupt_valid <= 1'b0;
          service_timer <= 0;
          timeout_error <= 1'b0;
          if ((pending_interrupts != 0) | interrupt_trig) begin
            current_state <= 1;
            // Compute effective priorities with starvation boost
            eff_pri_0 <= w_eff_pri_0;
            eff_pri_1 <= w_eff_pri_1;
            eff_pri_2 <= w_eff_pri_2;
            eff_pri_3 <= w_eff_pri_3;
            eff_pri_4 <= w_eff_pri_4;
            eff_pri_5 <= w_eff_pri_5;
            eff_pri_6 <= w_eff_pri_6;
            eff_pri_7 <= w_eff_pri_7;
            eff_pri_8 <= w_eff_pri_8;
            eff_pri_9 <= w_eff_pri_9;
            // Update wait counters for pending interrupts
            if ((pending_interrupts[0:0] == 1) | (interrupt_trig & (interrupt_requests[0:0] == 1))) begin
              wait_cnt_0 <= 4'(wait_cnt_0 + 1);
            end else begin
              wait_cnt_0 <= 0;
            end
            if ((pending_interrupts[1:1] == 1) | (interrupt_trig & (interrupt_requests[1:1] == 1))) begin
              wait_cnt_1 <= 4'(wait_cnt_1 + 1);
            end else begin
              wait_cnt_1 <= 0;
            end
            if ((pending_interrupts[2:2] == 1) | (interrupt_trig & (interrupt_requests[2:2] == 1))) begin
              wait_cnt_2 <= 4'(wait_cnt_2 + 1);
            end else begin
              wait_cnt_2 <= 0;
            end
            if ((pending_interrupts[3:3] == 1) | (interrupt_trig & (interrupt_requests[3:3] == 1))) begin
              wait_cnt_3 <= 4'(wait_cnt_3 + 1);
            end else begin
              wait_cnt_3 <= 0;
            end
            if ((pending_interrupts[4:4] == 1) | (interrupt_trig & (interrupt_requests[4:4] == 1))) begin
              wait_cnt_4 <= 4'(wait_cnt_4 + 1);
            end else begin
              wait_cnt_4 <= 0;
            end
            if ((pending_interrupts[5:5] == 1) | (interrupt_trig & (interrupt_requests[5:5] == 1))) begin
              wait_cnt_5 <= 4'(wait_cnt_5 + 1);
            end else begin
              wait_cnt_5 <= 0;
            end
            if ((pending_interrupts[6:6] == 1) | (interrupt_trig & (interrupt_requests[6:6] == 1))) begin
              wait_cnt_6 <= 4'(wait_cnt_6 + 1);
            end else begin
              wait_cnt_6 <= 0;
            end
            if ((pending_interrupts[7:7] == 1) | (interrupt_trig & (interrupt_requests[7:7] == 1))) begin
              wait_cnt_7 <= 4'(wait_cnt_7 + 1);
            end else begin
              wait_cnt_7 <= 0;
            end
            if ((pending_interrupts[8:8] == 1) | (interrupt_trig & (interrupt_requests[8:8] == 1))) begin
              wait_cnt_8 <= 4'(wait_cnt_8 + 1);
            end else begin
              wait_cnt_8 <= 0;
            end
            if ((pending_interrupts[9:9] == 1) | (interrupt_trig & (interrupt_requests[9:9] == 1))) begin
              wait_cnt_9 <= 4'(wait_cnt_9 + 1);
            end else begin
              wait_cnt_9 <= 0;
            end
          end
        end else if (current_state == 1) begin
          // PRIORITY_CALC - compute next interrupt id
          next_interrupt_id <= w_max_id;
          max_priority <= w_max_pri;
          current_state <= 2;
        end else if (current_state == 2) begin
          // SERVICE_PREP
          r_interrupt_id <= next_interrupt_id;
          r_interrupt_valid <= 1'b1;
          r_interrupt_status <= pending_interrupts;
          r_starvation_detected <= any_starvation;
          current_state <= 3;
          service_timer <= 0;
        end else if (current_state == 3) begin
          // SERVICING
          if (interrupt_ack) begin
            current_state <= 4;
          end else if (service_timer == 15) begin
            timeout_error <= 1'b1;
            current_state <= 5;
          end else begin
            service_timer <= 4'(service_timer + 1);
          end
        end else if (current_state == 4) begin
          // COMPLETION
          pending_interrupts <= pending_interrupts & ~(10'($unsigned(1)) << r_interrupt_id);
          r_interrupt_valid <= 1'b0;
          r_interrupt_status <= 0;
          current_state <= 0;
        end else begin
          // ERROR (state 5/7)
          r_interrupt_valid <= 1'b0;
          r_interrupt_status <= 0;
          current_state <= 0;
        end
      end
    end
  end
  // Output assignments
  assign interrupt_id = r_interrupt_id;
  assign interrupt_valid = r_interrupt_valid;
  assign interrupt_status = r_interrupt_status;
  assign missed_interrupts = r_missed_interrupts;
  assign starvation_detected = r_starvation_detected;

endmodule

