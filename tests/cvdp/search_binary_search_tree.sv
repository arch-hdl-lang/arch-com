module search_binary_search_tree #(
  parameter int DATA_WIDTH = 32,
  parameter int ARRAY_SIZE = 15,
  parameter int PTR_W = $clog2(ARRAY_SIZE) + 1
) (
  input logic clk,
  input logic reset,
  input logic start,
  input logic [DATA_WIDTH-1:0] search_key,
  input logic [PTR_W-1:0] root,
  input logic [ARRAY_SIZE * DATA_WIDTH-1:0] keys,
  input logic [ARRAY_SIZE * PTR_W-1:0] left_child,
  input logic [ARRAY_SIZE * PTR_W-1:0] right_child,
  output logic [PTR_W-1:0] key_position,
  output logic complete_found,
  output logic search_invalid
);

  // FSM: 0=S_IDLE,1=S_INIT,2=S_SEARCH_LEFT,3=S_SEARCH_LEFT_RIGHT,4=S_COMPLETE_SEARCH
  logic [3-1:0] search_state;
  // S_INIT takes 3 cycles: two warmup registers then process on third
  logic init_r1;
  logic init_r2;
  // found: key was found at root (need to count left subtree for position)
  logic found;
  // Left traversal state
  logic [PTR_W-1:0] cur_left;
  logic [ARRAY_SIZE * PTR_W-1:0] left_stack;
  logic [PTR_W-1:0] sp_left;
  logic [PTR_W-1:0] left_out_idx;
  // Right traversal state
  logic [PTR_W-1:0] cur_right;
  logic [ARRAY_SIZE * PTR_W-1:0] right_stack;
  logic [PTR_W-1:0] sp_right;
  logic [PTR_W-1:0] right_out_idx;
  // Phase 1 done flag and root position offset
  logic left_done;
  logic [PTR_W-1:0] pos;
  // Registered outputs
  logic [PTR_W-1:0] r_key_position;
  logic r_complete_found;
  logic r_search_invalid;
  logic [PTR_W-1:0] null_ptr;
  assign null_ptr = (1 << PTR_W) - 1;
  // Root fields (combinational)
  logic [DATA_WIDTH-1:0] root_key;
  logic [PTR_W-1:0] root_lc;
  logic [PTR_W-1:0] root_rc;
  // cur_left fields
  logic [DATA_WIDTH-1:0] cur_left_key;
  logic [PTR_W-1:0] cur_left_lc;
  logic [PTR_W-1:0] cur_left_rc;
  // Left stack top
  logic [PTR_W-1:0] sp_left_m1;
  logic [PTR_W-1:0] left_top_node;
  logic [DATA_WIDTH-1:0] left_top_key;
  logic [PTR_W-1:0] left_top_rc;
  // cur_right left child
  logic [PTR_W-1:0] cur_right_lc;
  // Right stack top
  logic [PTR_W-1:0] sp_right_m1;
  logic [PTR_W-1:0] right_top_node;
  logic [DATA_WIDTH-1:0] right_top_key;
  logic [PTR_W-1:0] right_top_rc;
  always_comb begin
    root_key = keys[root * DATA_WIDTH +: DATA_WIDTH];
    root_lc = left_child[root * PTR_W +: PTR_W];
    root_rc = right_child[root * PTR_W +: PTR_W];
    cur_left_key = keys[cur_left * DATA_WIDTH +: DATA_WIDTH];
    cur_left_lc = left_child[cur_left * PTR_W +: PTR_W];
    cur_left_rc = right_child[cur_left * PTR_W +: PTR_W];
    if (sp_left > 0) begin
      sp_left_m1 = PTR_W'(sp_left - 1);
    end else begin
      sp_left_m1 = 0;
    end
    left_top_node = left_stack[sp_left_m1 * PTR_W +: PTR_W];
    left_top_key = keys[left_top_node * DATA_WIDTH +: DATA_WIDTH];
    left_top_rc = right_child[left_top_node * PTR_W +: PTR_W];
    cur_right_lc = left_child[cur_right * PTR_W +: PTR_W];
    if (sp_right > 0) begin
      sp_right_m1 = PTR_W'(sp_right - 1);
    end else begin
      sp_right_m1 = 0;
    end
    right_top_node = right_stack[sp_right_m1 * PTR_W +: PTR_W];
    right_top_key = keys[right_top_node * DATA_WIDTH +: DATA_WIDTH];
    right_top_rc = right_child[right_top_node * PTR_W +: PTR_W];
    key_position = r_key_position;
    complete_found = r_complete_found;
    search_invalid = r_search_invalid;
  end
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      cur_left <= 0;
      cur_right <= 0;
      found <= 1'b0;
      init_r1 <= 1'b0;
      init_r2 <= 1'b0;
      left_done <= 1'b0;
      left_out_idx <= 0;
      left_stack <= 0;
      pos <= 0;
      r_complete_found <= 1'b0;
      r_key_position <= 0;
      r_search_invalid <= 1'b0;
      right_out_idx <= 0;
      right_stack <= 0;
      search_state <= 0;
      sp_left <= 0;
      sp_right <= 0;
    end else begin
      if (search_state == 0) begin
        // S_IDLE: wait for start
        r_complete_found <= 1'b0;
        r_search_invalid <= 1'b0;
        init_r1 <= 1'b0;
        init_r2 <= 1'b0;
        if (start) begin
          r_key_position <= null_ptr;
          left_out_idx <= 0;
          right_out_idx <= 0;
          sp_left <= 0;
          sp_right <= 0;
          left_done <= 1'b0;
          found <= 1'b0;
          pos <= 0;
          search_state <= 1;
        end
      end else if (search_state == 1) begin
        // S_INIT: 3 cycles (two warmup + one processing)
        if (init_r1 == 1'b0) begin
          init_r1 <= 1'b1;
        end else if (init_r2 == 1'b0) begin
          init_r2 <= 1'b1;
        end else begin
          init_r1 <= 1'b0;
          init_r2 <= 1'b0;
          if (root == null_ptr) begin
            // Empty tree
            r_key_position <= null_ptr;
            r_search_invalid <= 1'b1;
            r_complete_found <= 1'b0;
            search_state <= 4;
          end else if (root_key == search_key) begin
            if (root_lc == null_ptr) begin
              // Found at root with no left subtree: position = 0
              r_key_position <= 0;
              r_complete_found <= 1'b1;
              r_search_invalid <= 1'b0;
              search_state <= 4;
            end else begin
              // Found at root: count left subtree to determine position
              found <= 1'b1;
              cur_left <= root_lc;
              sp_left <= 0;
              left_out_idx <= 0;
              search_state <= 2;
            end
          end else if (search_key < root_key) begin
            // Search in left subtree
            found <= 1'b0;
            cur_left <= root_lc;
            sp_left <= 0;
            left_out_idx <= 0;
            search_state <= 2;
          end else begin
            // search_key > root_key: count left subtree + search right
            cur_left <= root_lc;
            cur_right <= root_rc;
            sp_left <= 0;
            sp_right <= 0;
            left_out_idx <= 0;
            right_out_idx <= 0;
            left_done <= 1'b0;
            search_state <= 3;
          end
        end
      end else if (search_state == 2) begin
        // S_SEARCH_LEFT: iterative in-order traversal
        if (cur_left != null_ptr) begin
          // Push current, go left
          left_stack[sp_left * PTR_W +: PTR_W] <= cur_left;
          sp_left <= PTR_W'(sp_left + 1);
          cur_left <= cur_left_lc;
        end else if (sp_left > 0) begin
          // Pop and check in one cycle
          sp_left <= sp_left_m1;
          cur_left <= left_top_rc;
          if (left_top_key == search_key) begin
            r_key_position <= left_out_idx;
            r_complete_found <= 1'b1;
            r_search_invalid <= 1'b0;
            search_state <= 4;
          end else begin
            left_out_idx <= PTR_W'(left_out_idx + 1);
          end
        end else begin
          // cur=null, sp=0: traversal complete
          if (found) begin
            r_key_position <= left_out_idx;
            r_complete_found <= 1'b1;
            r_search_invalid <= 1'b0;
          end else begin
            r_key_position <= null_ptr;
            r_complete_found <= 1'b0;
            r_search_invalid <= 1'b1;
          end
          search_state <= 4;
        end
      end else if (search_state == 3) begin
        // S_SEARCH_LEFT_RIGHT
        if (left_done == 1'b0) begin
          // Phase 1: count all nodes in root's left subtree
          if (cur_left != null_ptr) begin
            if (cur_left_lc != null_ptr) begin
              // Node has left child: push and go left
              left_stack[sp_left * PTR_W +: PTR_W] <= cur_left;
              sp_left <= PTR_W'(sp_left + 1);
              cur_left <= cur_left_lc;
            end else begin
              // Node has no left child: process directly (count + go right), no push
              // New left_out_idx = old+1. pos = (old+1)+1 = old+2.
              left_out_idx <= PTR_W'(left_out_idx + 1);
              cur_left <= cur_left_rc;
              if (cur_left_rc == null_ptr) begin
                if (sp_left == 0) begin
                  left_done <= 1'b1;
                  pos <= PTR_W'(left_out_idx + 2);
                end
              end
            end
          end else if (sp_left > 0) begin
            // Pop: count + go right
            sp_left <= sp_left_m1;
            left_out_idx <= PTR_W'(left_out_idx + 1);
            cur_left <= left_top_rc;
            if (left_top_rc == null_ptr) begin
              if (sp_left_m1 == 0) begin
                left_done <= 1'b1;
                pos <= PTR_W'(left_out_idx + 2);
              end
            end
          end else begin
            // cur=null, sp=0: counting done
            left_done <= 1'b1;
            pos <= PTR_W'(left_out_idx + 1);
          end
        end else if (cur_right != null_ptr) begin
          // Phase 2: search right subtree in-order
          // Push current, go left
          right_stack[sp_right * PTR_W +: PTR_W] <= cur_right;
          sp_right <= PTR_W'(sp_right + 1);
          cur_right <= cur_right_lc;
        end else if (sp_right > 0) begin
          // Pop and check in one cycle
          sp_right <= sp_right_m1;
          cur_right <= right_top_rc;
          if (right_top_key == search_key) begin
            r_key_position <= PTR_W'(pos + right_out_idx);
            r_complete_found <= 1'b1;
            r_search_invalid <= 1'b0;
            search_state <= 4;
          end else begin
            right_out_idx <= PTR_W'(right_out_idx + 1);
          end
        end else begin
          // cur=null, sp=0: key not in right subtree
          r_key_position <= null_ptr;
          r_complete_found <= 1'b0;
          r_search_invalid <= 1'b1;
          search_state <= 4;
        end
      end else if (search_state == 4) begin
        // S_COMPLETE_SEARCH: outputs already set, return to idle
        search_state <= 0;
      end
    end
  end

endmodule

