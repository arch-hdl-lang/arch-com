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

`ifdef SIMULATION
    localparam CLK_FREQ_MHZ = 100;
    localparam real SIM_DOOR_OPEN_TIME_MS = 0.05;
    localparam DOOR_OPEN_CYCLES = int'(SIM_DOOR_OPEN_TIME_MS * CLK_FREQ_MHZ * 1000);
`else
    localparam CLK_FREQ_MHZ = 100;
    localparam DOOR_OPEN_CYCLES = (DOOR_OPEN_TIME_MS * CLK_FREQ_MHZ * 1000);
`endif

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
