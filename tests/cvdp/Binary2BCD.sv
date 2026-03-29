module Binary2BCD (
  input logic [8-1:0] num,
  output logic [4-1:0] thousand,
  output logic [4-1:0] hundred,
  output logic [4-1:0] ten,
  output logic [4-1:0] one
);

  // Double-dabble: 20-bit shift register [19:16]=thousand [15:12]=hundred [11:8]=ten [7:0]=input
  logic [20-1:0] sh0;
  logic [20-1:0] sh1;
  logic [20-1:0] sh2;
  logic [20-1:0] sh3;
  logic [20-1:0] sh4;
  logic [20-1:0] sh5;
  logic [20-1:0] sh6;
  logic [20-1:0] sh7;
  logic [20-1:0] sh8;
  always_comb begin
    sh0 = 20'($unsigned(num));
    // Iteration 1: just shift (no BCD digits populated yet)
    sh1 = sh0 << 1;
    // Iteration 2
    sh2[7:0] = sh1[7:0];
    sh2[11:8] = sh1[11:8];
    if (sh1[11:8] >= 5) begin
      sh2[11:8] = 4'(sh1[11:8] + 3);
    end
    sh2[19:12] = sh1[19:12];
    sh2 = sh2 << 1;
    // Iteration 3
    sh3[7:0] = sh2[7:0];
    sh3[11:8] = sh2[11:8];
    if (sh2[11:8] >= 5) begin
      sh3[11:8] = 4'(sh2[11:8] + 3);
    end
    sh3[19:12] = sh2[19:12];
    sh3 = sh3 << 1;
    // Iteration 4
    sh4[7:0] = sh3[7:0];
    sh4[11:8] = sh3[11:8];
    if (sh3[11:8] >= 5) begin
      sh4[11:8] = 4'(sh3[11:8] + 3);
    end
    sh4[15:12] = sh3[15:12];
    if (sh3[15:12] >= 5) begin
      sh4[15:12] = 4'(sh3[15:12] + 3);
    end
    sh4[19:16] = sh3[19:16];
    sh4 = sh4 << 1;
    // Iteration 5
    sh5[7:0] = sh4[7:0];
    sh5[11:8] = sh4[11:8];
    if (sh4[11:8] >= 5) begin
      sh5[11:8] = 4'(sh4[11:8] + 3);
    end
    sh5[15:12] = sh4[15:12];
    if (sh4[15:12] >= 5) begin
      sh5[15:12] = 4'(sh4[15:12] + 3);
    end
    sh5[19:16] = sh4[19:16];
    sh5 = sh5 << 1;
    // Iteration 6
    sh6[7:0] = sh5[7:0];
    sh6[11:8] = sh5[11:8];
    if (sh5[11:8] >= 5) begin
      sh6[11:8] = 4'(sh5[11:8] + 3);
    end
    sh6[15:12] = sh5[15:12];
    if (sh5[15:12] >= 5) begin
      sh6[15:12] = 4'(sh5[15:12] + 3);
    end
    sh6[19:16] = sh5[19:16];
    sh6 = sh6 << 1;
    // Iteration 7
    sh7[7:0] = sh6[7:0];
    sh7[11:8] = sh6[11:8];
    if (sh6[11:8] >= 5) begin
      sh7[11:8] = 4'(sh6[11:8] + 3);
    end
    sh7[15:12] = sh6[15:12];
    if (sh6[15:12] >= 5) begin
      sh7[15:12] = 4'(sh6[15:12] + 3);
    end
    sh7[19:16] = sh6[19:16];
    sh7 = sh7 << 1;
    // Iteration 8
    sh8[7:0] = sh7[7:0];
    sh8[11:8] = sh7[11:8];
    if (sh7[11:8] >= 5) begin
      sh8[11:8] = 4'(sh7[11:8] + 3);
    end
    sh8[15:12] = sh7[15:12];
    if (sh7[15:12] >= 5) begin
      sh8[15:12] = 4'(sh7[15:12] + 3);
    end
    sh8[19:16] = sh7[19:16];
    sh8 = sh8 << 1;
    thousand = sh8[19:16];
    hundred = sh8[15:12];
    ten = sh8[11:8];
    one = sh8[7:4];
  end

endmodule

module floor_to_seven_segment (
  input logic clk,
  input logic [4-1:0] floor_display,
  output logic [7-1:0] seven_seg_out,
  output logic [4-1:0] seven_seg_out_anode,
  output logic [4-1:0] thousand,
  output logic [4-1:0] hundred,
  output logic [4-1:0] ten,
  output logic [4-1:0] one
);

  // BCD conversion
  logic [4-1:0] bcd_thou;
  logic [4-1:0] bcd_hund;
  logic [4-1:0] bcd_ten;
  logic [4-1:0] bcd_one;
  Binary2BCD u_bcd (
    .num(8'($unsigned(floor_display))),
    .thousand(bcd_thou),
    .hundred(bcd_hund),
    .ten(bcd_ten),
    .one(bcd_one)
  );
  assign thousand = bcd_thou;
  assign hundred = bcd_hund;
  assign ten = bcd_ten;
  assign one = bcd_one;
  // 2-bit counter for digit mux
  logic [2-1:0] digit_sel;
  always_ff @(posedge clk) begin
    digit_sel <= 2'(digit_sel + 1);
  end
  // Select which digit to display and which anode to activate
  logic [4-1:0] current_digit;
  always_comb begin
    if (digit_sel == 0) begin
      current_digit = bcd_one;
      seven_seg_out_anode = 14;
    end else if (digit_sel == 1) begin
      current_digit = bcd_ten;
      seven_seg_out_anode = 13;
    end else if (digit_sel == 2) begin
      current_digit = bcd_hund;
      seven_seg_out_anode = 11;
    end else begin
      current_digit = bcd_thou;
      seven_seg_out_anode = 7;
    end
  end
  // Seven-segment decoder
  always_comb begin
    if (current_digit == 0) begin
      seven_seg_out = 126;
    end else if (current_digit == 1) begin
      seven_seg_out = 48;
    end else if (current_digit == 2) begin
      seven_seg_out = 109;
    end else if (current_digit == 3) begin
      seven_seg_out = 121;
    end else if (current_digit == 4) begin
      seven_seg_out = 51;
    end else if (current_digit == 5) begin
      seven_seg_out = 91;
    end else if (current_digit == 6) begin
      seven_seg_out = 95;
    end else if (current_digit == 7) begin
      seven_seg_out = 112;
    end else if (current_digit == 8) begin
      seven_seg_out = 127;
    end else if (current_digit == 9) begin
      seven_seg_out = 123;
    end else begin
      seven_seg_out = 0;
    end
  end

endmodule

module elevator_control_system #(
    parameter N = 8,
    parameter DOOR_OPEN_TIME_MS = 500
) (
    input wire clk,
    input wire reset,
    input wire [N-1:0] call_requests,
    input wire emergency_stop,
    input wire overload_detected,
    output wire [$clog2(N)-1:0] current_floor,
    output reg direction,
    output reg door_open,
    output reg [2:0] system_status,
    output overload_warning,
    output wire [6:0] seven_seg_out,
    output wire [3:0] seven_seg_out_anode,
    output wire [3:0] thousand,
    output wire [3:0] hundred,
    output wire [3:0] ten,
    output wire [3:0] one
);

// State Encoding
localparam IDLE = 3'b000;
localparam MOVING_UP = 3'b001;
localparam MOVING_DOWN = 3'b010;
localparam EMERGENCY_HALT = 3'b011;
localparam DOOR_OPEN_ST = 3'b100;
localparam OVERLOAD_HALT = 3'b101;

// Internal registers
reg [N-1:0] call_requests_internal;
reg [2:0] present_state, next_state;
reg [$clog2(N)-1:0] max_request;
reg [$clog2(N)-1:0] min_request;

localparam CLK_FREQ_MHZ = 100;
localparam DOOR_OPEN_CYCLES = 5000;

reg [$clog2(DOOR_OPEN_CYCLES)-1:0] door_open_counter;

reg [$clog2(N)-1:0] current_floor_reg, current_floor_next;

assign current_floor = current_floor_reg;
assign overload_warning = (overload_detected == 1 && present_state == OVERLOAD_HALT);

// FSM state transition
always @(posedge clk or posedge reset) begin
    if (reset) begin
        present_state <= IDLE;
        system_status <= IDLE;
        current_floor_reg <= 0;
        max_request <= 0;
        min_request <= N-1;
    end else begin
        present_state <= next_state;
        system_status <= next_state;
        current_floor_reg <= current_floor_next;

        max_request = 0;
        min_request = N-1;
        for (integer i = 0; i < N; i = i + 1) begin
            if (call_requests_internal[i]) begin
                if (i > max_request) max_request = i;
                if (i < min_request) min_request = i;
            end
        end
    end
end

// Next state logic
always @(*) begin
    next_state = present_state;
    current_floor_next = current_floor_reg;

    case (present_state)
        IDLE: begin
            if (overload_detected) begin
                next_state = OVERLOAD_HALT;
            end else if (emergency_stop) begin
                next_state = EMERGENCY_HALT;
            end else if (call_requests_internal != 0) begin
                if (max_request > current_floor_reg) begin
                    next_state = MOVING_UP;
                end else if (min_request < current_floor_reg) begin
                    next_state = MOVING_DOWN;
                end
            end
        end

        MOVING_UP: begin
            if (emergency_stop) begin
                next_state = EMERGENCY_HALT;
            end else if (call_requests_internal[current_floor_reg+1]) begin
                current_floor_next = current_floor_reg + 1;
                next_state = DOOR_OPEN_ST;
            end else if (current_floor_reg >= max_request) begin
                next_state = IDLE;
            end else begin
                current_floor_next = current_floor_reg + 1;
                next_state = MOVING_UP;
            end
        end

        MOVING_DOWN: begin
            if (emergency_stop) begin
                next_state = EMERGENCY_HALT;
            end else if (call_requests_internal[current_floor_reg-1]) begin
                current_floor_next = current_floor_reg - 1;
                next_state = DOOR_OPEN_ST;
            end else if (current_floor_reg <= min_request) begin
                next_state = IDLE;
            end else begin
                current_floor_next = current_floor_reg - 1;
                next_state = MOVING_DOWN;
            end
        end

        EMERGENCY_HALT: begin
            if (!emergency_stop) begin
                next_state = IDLE;
                current_floor_next = 0;
            end
        end

        DOOR_OPEN_ST: begin
            if (overload_detected) begin
                next_state = OVERLOAD_HALT;
            end else if (door_open_counter == 0) begin
                next_state = IDLE;
            end else begin
                next_state = DOOR_OPEN_ST;
            end
        end

        OVERLOAD_HALT: begin
            if (!overload_detected) begin
                if (door_open) begin
                    next_state = DOOR_OPEN_ST;
                end else begin
                    next_state = IDLE;
                end
            end
        end
    endcase
end

// Door open control
always @(posedge clk or posedge reset) begin
    if (reset) begin
        door_open_counter <= 0;
        door_open <= 0;
    end else begin
        if (present_state == OVERLOAD_HALT) begin
            door_open_counter <= DOOR_OPEN_CYCLES;
            door_open <= 1;
        end else if (present_state == DOOR_OPEN_ST) begin
            if (door_open_counter > 0) begin
                door_open <= 1;
                door_open_counter <= door_open_counter - 1;
            end else begin
                door_open <= 0;
            end
        end else begin
            door_open <= 0;
            door_open_counter <= DOOR_OPEN_CYCLES;
        end
    end
end

// Call request management
always @(*) begin
    if (reset) begin
        call_requests_internal = 0;
    end else begin
        if (call_requests_internal[current_floor_reg]) begin
            call_requests_internal[current_floor_reg] = 0;
        end
        call_requests_internal = call_requests_internal | call_requests;
    end
end

// Direction control
always @(*) begin
    if (reset) begin
        direction = 1;
    end else begin
        if (present_state == MOVING_UP) begin
            direction = 1;
        end else if (present_state == MOVING_DOWN) begin
            direction = 0;
        end else begin
            direction = 1;
        end
    end
end

// Seven-segment display
floor_to_seven_segment floor_display_converter (
    .clk(clk),
    .floor_display(current_floor_reg),
    .seven_seg_out(seven_seg_out),
    .seven_seg_out_anode(seven_seg_out_anode),
    .thousand(thousand),
    .hundred(hundred),
    .ten(ten),
    .one(one)
);

endmodule
