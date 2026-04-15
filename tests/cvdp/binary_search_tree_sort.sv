module binary_search_tree_sort #(
  parameter int DATA_WIDTH = 32,
  parameter int ARRAY_SIZE = 8,
  parameter int PTR_W = $clog2(ARRAY_SIZE) + 1
) (
  input logic clk,
  input logic reset,
  input logic [ARRAY_SIZE * DATA_WIDTH-1:0] data_in,
  input logic start,
  output logic [ARRAY_SIZE * DATA_WIDTH-1:0] sorted_out,
  output logic done
);

  // Top-level FSM states: 0=IDLE, 1=BUILD_TREE, 2=SORT_TREE
  logic [1:0] top_state;
  // Build FSM states: 0=INIT, 1=CHECK_ROOT, 2=TRAVERSE, 4=COMPLETE
  logic [2:0] build_state;
  // Sort FSM states:
  // 0=S_INIT, 1=TRAVERSE_LEFT, 2=POP_PROCESS, 3=ASSIGN_RIGHT, 4=CHECK_RIGHT, 5=S_COMPLETE
  logic [2:0] sort_state;
  // BST representation
  logic [ARRAY_SIZE * DATA_WIDTH-1:0] keys;
  logic [ARRAY_SIZE * PTR_W-1:0] left_child;
  logic [ARRAY_SIZE * PTR_W-1:0] right_child;
  logic [PTR_W-1:0] root;
  logic [PTR_W-1:0] next_free_node;
  // Stack for in-order traversal
  logic [ARRAY_SIZE * PTR_W-1:0] stack_mem;
  logic [PTR_W-1:0] sp;
  // Working registers
  logic [PTR_W-1:0] current_node;
  logic [PTR_W-1:0] input_index;
  logic [PTR_W-1:0] output_index;
  logic [DATA_WIDTH-1:0] temp_data;
  logic [ARRAY_SIZE * DATA_WIDTH-1:0] r_sorted_out;
  logic r_done;
  logic [PTR_W-1:0] null_ptr;
  assign null_ptr = (1 << PTR_W) - 1;
  // Combinational extraction of current_node fields
  logic [DATA_WIDTH-1:0] cur_key;
  logic [PTR_W-1:0] cur_left;
  logic [PTR_W-1:0] cur_right;
  // Stack top (safe when sp>0)
  logic [PTR_W-1:0] sp_m1;
  logic [PTR_W-1:0] stack_top_ptr;
  logic [DATA_WIDTH-1:0] stack_top_key;
  always_comb begin
    cur_key = keys[current_node * DATA_WIDTH +: DATA_WIDTH];
    cur_left = left_child[current_node * PTR_W +: PTR_W];
    cur_right = right_child[current_node * PTR_W +: PTR_W];
    if (sp > 0) begin
      sp_m1 = PTR_W'(sp - 1);
    end else begin
      sp_m1 = 0;
    end
    stack_top_ptr = stack_mem[sp_m1 * PTR_W +: PTR_W];
    stack_top_key = keys[stack_top_ptr * DATA_WIDTH +: DATA_WIDTH];
  end
  assign sorted_out = r_sorted_out;
  assign done = r_done;
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      build_state <= 0;
      current_node <= 0;
      input_index <= 0;
      keys <= 0;
      left_child <= 0;
      next_free_node <= 0;
      output_index <= 0;
      r_done <= 1'b0;
      r_sorted_out <= 0;
      right_child <= 0;
      root <= 0;
      sort_state <= 0;
      sp <= 0;
      stack_mem <= 0;
      temp_data <= 0;
      top_state <= 0;
    end else begin
      if (top_state == 0) begin
        // IDLE
        r_done <= 1'b0;
        r_sorted_out <= 0;
        input_index <= 0;
        output_index <= 0;
        root <= null_ptr;
        next_free_node <= 0;
        sp <= 0;
        keys <= 0;
        for (int i = 0; i <= ARRAY_SIZE - 1; i++) begin
          left_child[i * PTR_W +: PTR_W] <= null_ptr;
          right_child[i * PTR_W +: PTR_W] <= null_ptr;
          stack_mem[i * PTR_W +: PTR_W] <= null_ptr;
        end
        if (start) begin
          top_state <= 1;
          build_state <= 0;
        end
      end else if (top_state == 1) begin
        // BUILD_TREE
        if (build_state == 0) begin
          // INIT
          if (32'($unsigned(input_index)) < ARRAY_SIZE) begin
            temp_data <= data_in[input_index * DATA_WIDTH +: DATA_WIDTH];
            input_index <= PTR_W'(input_index + 1);
            build_state <= 1;
          end else begin
            build_state <= 4;
          end
        end else if (build_state == 1) begin
          // CHECK_ROOT
          if (root == null_ptr) begin
            keys[next_free_node * DATA_WIDTH +: DATA_WIDTH] <= temp_data;
            left_child[next_free_node * PTR_W +: PTR_W] <= null_ptr;
            right_child[next_free_node * PTR_W +: PTR_W] <= null_ptr;
            root <= next_free_node;
            next_free_node <= PTR_W'(next_free_node + 1);
            build_state <= 0;
          end else begin
            current_node <= root;
            build_state <= 2;
          end
        end else if (build_state == 2) begin
          // TRAVERSE
          if (temp_data > cur_key) begin
            if (cur_right == null_ptr) begin
              keys[next_free_node * DATA_WIDTH +: DATA_WIDTH] <= temp_data;
              left_child[next_free_node * PTR_W +: PTR_W] <= null_ptr;
              right_child[next_free_node * PTR_W +: PTR_W] <= null_ptr;
              right_child[current_node * PTR_W +: PTR_W] <= next_free_node;
              next_free_node <= PTR_W'(next_free_node + 1);
              build_state <= 0;
            end else begin
              current_node <= cur_right;
            end
          end else if (cur_left == null_ptr) begin
            keys[next_free_node * DATA_WIDTH +: DATA_WIDTH] <= temp_data;
            left_child[next_free_node * PTR_W +: PTR_W] <= null_ptr;
            right_child[next_free_node * PTR_W +: PTR_W] <= null_ptr;
            left_child[current_node * PTR_W +: PTR_W] <= next_free_node;
            next_free_node <= PTR_W'(next_free_node + 1);
            build_state <= 0;
          end else begin
            current_node <= cur_left;
          end
        end else if (build_state == 4) begin
          // COMPLETE
          top_state <= 2;
          sort_state <= 0;
        end
      end else if (top_state == 2) begin
        // SORT_TREE
        if (sort_state == 0) begin
          // S_INIT
          if (root != null_ptr) begin
            current_node <= root;
            sort_state <= 1;
          end else begin
            sort_state <= 5;
          end
        end else if (sort_state == 1) begin
          // TRAVERSE_LEFT: push current, go left if possible
          stack_mem[sp * PTR_W +: PTR_W] <= current_node;
          sp <= PTR_W'(sp + 1);
          if (cur_left != null_ptr) begin
            current_node <= cur_left;
          end else begin
            // No left child - go directly to pop
            sort_state <= 2;
          end
        end else if (sort_state == 2) begin
          // POP_PROCESS: pop and output
          if (sp > 0) begin
            current_node <= stack_top_ptr;
            r_sorted_out[output_index * DATA_WIDTH +: DATA_WIDTH] <= stack_top_key;
            output_index <= PTR_W'(output_index + 1);
            sp <= PTR_W'(sp - 1);
            sort_state <= 3;
          end else begin
            sort_state <= 5;
          end
        end else if (sort_state == 3) begin
          // ASSIGN_RIGHT
          current_node <= cur_right;
          sort_state <= 4;
        end else if (sort_state == 4) begin
          // CHECK_RIGHT
          if (current_node != null_ptr) begin
            sort_state <= 1;
          end else begin
            // No right child - always go to POP (even if sp=0, POP will route to DONE)
            sort_state <= 2;
          end
        end else if (sort_state == 5) begin
          // S_COMPLETE
          r_done <= 1'b1;
          top_state <= 0;
        end
      end
    end
  end

endmodule

