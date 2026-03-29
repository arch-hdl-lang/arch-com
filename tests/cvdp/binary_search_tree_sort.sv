// BST-based sorting: HFSM with BUILD_TREE and SORT_TREE sub-FSMs
// Cycle-exact implementation matching spec latency requirements:
//   BUILD: 2*N + ((N-1)*N/2) + 2  (sorted worst case)
//   SORT:  4*N + 3
module binary_search_tree_sort #(
    parameter DATA_WIDTH = 32,
    parameter ARRAY_SIZE = 8
) (
    input  wire                              clk,
    input  wire                              reset,
    input  wire [ARRAY_SIZE*DATA_WIDTH-1:0]  data_in,
    input  wire                              start,
    output reg  [ARRAY_SIZE*DATA_WIDTH-1:0]  sorted_out,
    output reg                               done
);

    // PTR_W: enough bits for node indices 0..ARRAY_SIZE-1 plus a NULL sentinel
    localparam PTR_W = $clog2(ARRAY_SIZE) + 1;

    // Top-level FSM
    localparam [1:0] S_IDLE  = 2'd0,
                     S_BUILD = 2'd1,
                     S_SORT  = 2'd2;

    // BUILD sub-states
    // Cycle cost per element: 2 (B_INIT+B_INSERT for root, or B_INIT+B_INSERT+N*B_COMPARE)
    // B_INIT:    check array bounds + load element → B_INSERT (or → B_COMPLETE)
    // B_INSERT:  if root=NULL: insert as root → B_INIT (1 cycle)
    //            else: set cur=root → B_COMPARE (1 cycle)
    // B_COMPARE: compare temp vs keys[cur], descend or insert (stays here for depth)
    // B_COMPLETE:transition to sort
    localparam [1:0] B_INIT     = 2'd0,
                     B_INSERT   = 2'd1,
                     B_COMPARE  = 2'd2,
                     B_COMPLETE = 2'd3;

    // SORT sub-states
    // Total cycle cost for sorted/dup input: 4*N + 3
    //   T_INIT(1) + T_INIT2(1): 2 cycles to start
    //   T_PUSHLEFT: N cycles for left traversal (or 1 per right-child node)
    //   T_POP + T_GORIGHT + T_CHECKRIGHT: 3 per node × N nodes
    //   T_DONE: 1 cycle
    localparam [2:0] T_INIT       = 3'd0,  // set cur = root
                     T_INIT2      = 3'd1,  // extra cycle → T_PUSHLEFT
                     T_PUSHLEFT   = 3'd2,  // push cur; go left if exists; else → T_POP
                     T_POP        = 3'd3,  // pop stack, output key
                     T_GORIGHT    = 3'd4,  // cur ← right child of popped node
                     T_CHECKRIGHT = 3'd5,  // right exists → T_PUSHLEFT; else pop or done
                     T_DONE       = 3'd6;  // assert done for 1 cycle

    reg [1:0] top_state;
    reg [1:0] bst_state;
    reg [2:0] srt_state;

    // BST node storage (unpacked arrays for clean per-element access)
    reg [DATA_WIDTH-1:0] keys   [0:ARRAY_SIZE-1];
    reg [PTR_W-1:0]      lchild [0:ARRAY_SIZE-1];
    reg [PTR_W-1:0]      rchild [0:ARRAY_SIZE-1];
    reg [PTR_W-1:0]      root_ptr;
    reg [PTR_W-1:0]      nfree;

    // Traversal stack
    reg [PTR_W-1:0]      stk    [0:ARRAY_SIZE-1];
    reg [PTR_W-1:0]      sp;

    // Working registers
    reg [PTR_W-1:0]      cur;
    reg [PTR_W-1:0]      in_idx;
    reg [PTR_W-1:0]      out_idx;
    reg [DATA_WIDTH-1:0] temp;

    // Latched input (captured when start is asserted)
    reg [ARRAY_SIZE*DATA_WIDTH-1:0] din_lat;

    // Safe PTR_W-wide 1 and NULL constants
    wire [PTR_W-1:0] ONE  = {{(PTR_W-1){1'b0}}, 1'b1};
    wire [PTR_W-1:0] NPTR = {PTR_W{1'b1}};

    integer i;

    always @(posedge clk or posedge reset) begin
        if (reset) begin
            top_state <= S_IDLE;
            bst_state <= B_INIT;
            srt_state <= T_INIT;
            root_ptr  <= {PTR_W{1'b1}};
            nfree     <= {PTR_W{1'b0}};
            sp        <= {PTR_W{1'b0}};
            in_idx    <= {PTR_W{1'b0}};
            out_idx   <= {PTR_W{1'b0}};
            done      <= 1'b0;
            sorted_out<= {(ARRAY_SIZE*DATA_WIDTH){1'b0}};
            din_lat   <= {(ARRAY_SIZE*DATA_WIDTH){1'b0}};
            temp      <= {DATA_WIDTH{1'b0}};
            cur       <= {PTR_W{1'b1}};
            for (i = 0; i < ARRAY_SIZE; i = i + 1) begin
                keys[i]   <= {DATA_WIDTH{1'b0}};
                lchild[i] <= {PTR_W{1'b1}};
                rchild[i] <= {PTR_W{1'b1}};
                stk[i]    <= {PTR_W{1'b1}};
            end
        end else begin
            case (top_state)

                // ============================================================
                S_IDLE: begin
                    done       <= 1'b0;
                    sorted_out <= {(ARRAY_SIZE*DATA_WIDTH){1'b0}};
                    if (start) begin
                        din_lat   <= data_in;
                        in_idx    <= {PTR_W{1'b0}};
                        out_idx   <= {PTR_W{1'b0}};
                        root_ptr  <= {PTR_W{1'b1}};
                        nfree     <= {PTR_W{1'b0}};
                        sp        <= {PTR_W{1'b0}};
                        for (i = 0; i < ARRAY_SIZE; i = i + 1) begin
                            keys[i]   <= {DATA_WIDTH{1'b0}};
                            lchild[i] <= {PTR_W{1'b1}};
                            rchild[i] <= {PTR_W{1'b1}};
                            stk[i]    <= {PTR_W{1'b1}};
                        end
                        top_state <= S_BUILD;
                        bst_state <= B_INIT;
                    end
                end

                // ============================================================
                S_BUILD: begin
                    case (bst_state)

                        // Load next element or go to COMPLETE
                        B_INIT: begin
                            if (in_idx < ARRAY_SIZE[PTR_W-1:0]) begin
                                temp      <= din_lat[in_idx * DATA_WIDTH +: DATA_WIDTH];
                                in_idx    <= in_idx + ONE;
                                bst_state <= B_INSERT;
                            end else begin
                                bst_state <= B_COMPLETE;
                            end
                        end

                        // Insert as root or start tree traversal
                        B_INSERT: begin
                            if (root_ptr == {PTR_W{1'b1}}) begin
                                keys[nfree]   <= temp;
                                lchild[nfree] <= {PTR_W{1'b1}};
                                rchild[nfree] <= {PTR_W{1'b1}};
                                root_ptr      <= nfree;
                                nfree         <= nfree + ONE;
                                bst_state     <= B_INIT;
                            end else begin
                                cur       <= root_ptr;
                                bst_state <= B_COMPARE;
                            end
                        end

                        // Compare and descend/insert (stays here for tree depth)
                        B_COMPARE: begin
                            if (temp > keys[cur]) begin
                                if (rchild[cur] == {PTR_W{1'b1}}) begin
                                    keys[nfree]   <= temp;
                                    lchild[nfree] <= {PTR_W{1'b1}};
                                    rchild[nfree] <= {PTR_W{1'b1}};
                                    rchild[cur]   <= nfree;
                                    nfree         <= nfree + ONE;
                                    bst_state     <= B_INIT;
                                end else begin
                                    cur <= rchild[cur];
                                end
                            end else begin
                                if (lchild[cur] == {PTR_W{1'b1}}) begin
                                    keys[nfree]   <= temp;
                                    lchild[nfree] <= {PTR_W{1'b1}};
                                    rchild[nfree] <= {PTR_W{1'b1}};
                                    lchild[cur]   <= nfree;
                                    nfree         <= nfree + ONE;
                                    bst_state     <= B_INIT;
                                end else begin
                                    cur <= lchild[cur];
                                end
                            end
                        end

                        // All elements inserted → begin sort
                        B_COMPLETE: begin
                            top_state <= S_SORT;
                            srt_state <= T_INIT;
                            out_idx   <= {PTR_W{1'b0}};
                            sp        <= {PTR_W{1'b0}};
                        end

                        default: bst_state <= B_INIT;
                    endcase
                end

                // ============================================================
                S_SORT: begin
                    case (srt_state)

                        // T_INIT: set cur = root (1st of 2 init cycles)
                        T_INIT: begin
                            if (root_ptr != {PTR_W{1'b1}}) begin
                                cur       <= root_ptr;
                                srt_state <= T_INIT2;
                            end else begin
                                srt_state <= T_DONE;
                            end
                        end

                        // T_INIT2: 2nd init cycle → proceed to left traversal
                        T_INIT2: begin
                            srt_state <= T_PUSHLEFT;
                        end

                        // T_PUSHLEFT: push cur; descend left if possible, else → T_POP
                        // (no separate T_LEFTDONE state — T_PUSHLEFT directly → T_POP
                        //  when left child is NULL)
                        T_PUSHLEFT: begin
                            stk[sp] <= cur;
                            sp      <= sp + ONE;
                            if (lchild[cur] != {PTR_W{1'b1}}) begin
                                cur <= lchild[cur];
                                // stay in T_PUSHLEFT
                            end else begin
                                srt_state <= T_POP;
                            end
                        end

                        // T_POP: pop stack, record output key, move cur to popped node
                        T_POP: begin
                            if (sp != {PTR_W{1'b0}}) begin
                                sp      <= sp - ONE;
                                cur     <= stk[sp - ONE];
                                sorted_out[out_idx * DATA_WIDTH +: DATA_WIDTH]
                                    <= keys[stk[sp - ONE]];
                                out_idx   <= out_idx + ONE;
                                srt_state <= T_GORIGHT;
                            end else begin
                                srt_state <= T_DONE;
                            end
                        end

                        // T_GORIGHT: advance cur to right child of popped node
                        T_GORIGHT: begin
                            cur       <= rchild[cur];
                            srt_state <= T_CHECKRIGHT;
                        end

                        // T_CHECKRIGHT: right exists → T_PUSHLEFT; else pop or done
                        T_CHECKRIGHT: begin
                            if (cur != {PTR_W{1'b1}}) begin
                                srt_state <= T_PUSHLEFT;
                            end else if (sp != {PTR_W{1'b0}}) begin
                                srt_state <= T_POP;
                            end else begin
                                srt_state <= T_DONE;
                            end
                        end

                        // T_DONE: assert done for exactly 1 cycle, return to IDLE
                        T_DONE: begin
                            done      <= 1'b1;
                            top_state <= S_IDLE;
                        end

                        default: srt_state <= T_INIT;
                    endcase
                end

                default: top_state <= S_IDLE;
            endcase
        end
    end

endmodule
