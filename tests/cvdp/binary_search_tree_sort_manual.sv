module binary_search_tree_sort #(
    parameter DATA_WIDTH = 32,
    parameter ARRAY_SIZE = 8
) (
    input clk,
    input reset,
    input [ARRAY_SIZE*DATA_WIDTH-1:0] data_in,
    input start,
    output reg [ARRAY_SIZE*DATA_WIDTH-1:0] sorted_out,
    output reg done
);

    localparam PTR_W = $clog2(ARRAY_SIZE) + 1;
    localparam [PTR_W-1:0] NULL_PTR = {PTR_W{1'b1}};

    // FSM states
    localparam [1:0] IDLE = 0, BUILD_TREE = 1, SORT_TREE = 2;
    localparam [2:0] B_INIT = 0, B_CHECK = 1, B_TRAVERSE = 2, B_COMPLETE = 4;
    localparam [2:0] S_INIT = 0, S_PUSH = 1, S_LEFT_NULL = 2, S_POP = 3, S_ASGN_R = 4, S_CHK_R = 5, S_DONE = 6;

    reg [1:0] top_state;
    reg [2:0] build_state, sort_state;

    // Packed arrays (matching original spec structure)
    reg [ARRAY_SIZE*DATA_WIDTH-1:0] keys;
    reg [ARRAY_SIZE*PTR_W-1:0] left_child;
    reg [ARRAY_SIZE*PTR_W-1:0] right_child;
    reg [ARRAY_SIZE*PTR_W-1:0] stack_mem;

    reg [PTR_W-1:0] root, next_free, sp, current_node;
    reg [PTR_W-1:0] input_index, output_index;
    reg [DATA_WIDTH-1:0] temp_data;

    integer i;

    // Helper: read from packed array safely
    wire [DATA_WIDTH-1:0] cur_key;
    wire [PTR_W-1:0] cur_left, cur_right;

    assign cur_key   = keys[current_node * DATA_WIDTH +: DATA_WIDTH];
    assign cur_left  = left_child[current_node * PTR_W +: PTR_W];
    assign cur_right = right_child[current_node * PTR_W +: PTR_W];

    // Stack top
    wire [PTR_W-1:0] sp_minus_one;
    wire [PTR_W-1:0] stk_top_node;
    wire [DATA_WIDTH-1:0] stk_top_key;

    assign sp_minus_one = (sp > 0) ? (sp - 1'd1) : {PTR_W{1'b0}};
    assign stk_top_node = stack_mem[sp_minus_one * PTR_W +: PTR_W];
    assign stk_top_key  = keys[stk_top_node * DATA_WIDTH +: DATA_WIDTH];

    always @(posedge clk or posedge reset) begin
        if (reset) begin
            top_state <= IDLE;
            build_state <= B_INIT;
            sort_state <= S_INIT;
            root <= NULL_PTR;
            next_free <= 0;
            sp <= 0;
            input_index <= 0;
            output_index <= 0;
            done <= 0;
            sorted_out <= 0;
            temp_data <= 0;
            current_node <= 0;
            keys <= 0;
            left_child  <= {ARRAY_SIZE{NULL_PTR}};
            right_child <= {ARRAY_SIZE{NULL_PTR}};
            stack_mem   <= {ARRAY_SIZE{NULL_PTR}};
        end else begin
            case (top_state)
                IDLE: begin
                    done <= 0;
                    sorted_out <= 0;
                    input_index <= 0;
                    output_index <= 0;
                    root <= NULL_PTR;
                    next_free <= 0;
                    sp <= 0;
                    keys <= 0;
                    left_child  <= {ARRAY_SIZE{NULL_PTR}};
                    right_child <= {ARRAY_SIZE{NULL_PTR}};
                    stack_mem   <= {ARRAY_SIZE{NULL_PTR}};
                    if (start) begin
                        top_state <= BUILD_TREE;
                        build_state <= B_INIT;
                    end
                end

                BUILD_TREE: begin
                    case (build_state)
                        B_INIT: begin
                            if (input_index < ARRAY_SIZE[PTR_W-1:0]) begin
                                temp_data <= data_in[input_index * DATA_WIDTH +: DATA_WIDTH];
                                input_index <= input_index + 1'd1;
                                build_state <= B_CHECK;
                            end else begin
                                build_state <= B_COMPLETE;
                            end
                        end

                        B_CHECK: begin
                            if (root == NULL_PTR) begin
                                keys[next_free * DATA_WIDTH +: DATA_WIDTH] <= temp_data;
                                left_child[next_free * PTR_W +: PTR_W] <= NULL_PTR;
                                right_child[next_free * PTR_W +: PTR_W] <= NULL_PTR;
                                root <= next_free;
                                next_free <= next_free + 1'd1;
                                build_state <= B_INIT;
                            end else begin
                                current_node <= root;
                                build_state <= B_TRAVERSE;
                            end
                        end

                        B_TRAVERSE: begin
                            if (temp_data > cur_key) begin
                                if (cur_right == NULL_PTR) begin
                                    keys[next_free * DATA_WIDTH +: DATA_WIDTH] <= temp_data;
                                    left_child[next_free * PTR_W +: PTR_W] <= NULL_PTR;
                                    right_child[next_free * PTR_W +: PTR_W] <= NULL_PTR;
                                    right_child[current_node * PTR_W +: PTR_W] <= next_free;
                                    next_free <= next_free + 1'd1;
                                    build_state <= B_INIT;
                                end else begin
                                    current_node <= cur_right;
                                end
                            end else begin
                                if (cur_left == NULL_PTR) begin
                                    keys[next_free * DATA_WIDTH +: DATA_WIDTH] <= temp_data;
                                    left_child[next_free * PTR_W +: PTR_W] <= NULL_PTR;
                                    right_child[next_free * PTR_W +: PTR_W] <= NULL_PTR;
                                    left_child[current_node * PTR_W +: PTR_W] <= next_free;
                                    next_free <= next_free + 1'd1;
                                    build_state <= B_INIT;
                                end else begin
                                    current_node <= cur_left;
                                end
                            end
                        end

                        B_COMPLETE: begin
                            top_state <= SORT_TREE;
                            sort_state <= S_INIT;
                        end
                    endcase
                end

                SORT_TREE: begin
                    case (sort_state)
                        S_INIT: begin
                            if (root != NULL_PTR) begin
                                current_node <= root;
                                sort_state <= S_PUSH;
                            end else begin
                                sort_state <= S_DONE;
                            end
                        end

                        S_PUSH: begin
                            stack_mem[sp * PTR_W +: PTR_W] <= current_node;
                            sp <= sp + 1'd1;
                            if (cur_left != NULL_PTR) begin
                                current_node <= cur_left;
                            end else begin
                                sort_state <= S_LEFT_NULL;
                            end
                        end

                        S_LEFT_NULL: begin
                            sort_state <= S_POP;
                        end

                        S_POP: begin
                            if (sp > 0) begin
                                current_node <= stk_top_node;
                                sorted_out[output_index * DATA_WIDTH +: DATA_WIDTH] <= stk_top_key;
                                output_index <= output_index + 1'd1;
                                sp <= sp - 1'd1;
                                sort_state <= S_ASGN_R;
                            end else begin
                                sort_state <= S_DONE;
                            end
                        end

                        S_ASGN_R: begin
                            current_node <= cur_right;
                            sort_state <= S_CHK_R;
                        end

                        S_CHK_R: begin
                            if (current_node != NULL_PTR) begin
                                sort_state <= S_PUSH;
                            end else begin
                                if (sp > 0) begin
                                    sort_state <= S_POP;
                                end else begin
                                    sort_state <= S_DONE;
                                end
                            end
                        end

                        S_DONE: begin
                            done <= 1;
                            top_state <= IDLE;
                        end
                    endcase
                end
            endcase
        end
    end
endmodule
