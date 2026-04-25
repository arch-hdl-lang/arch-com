// 3-stage credit_channel forwarding chain — see doc/plan_credit_channel.md
// §"Stretch: 4×4 mesh" for the broader vision. This is the minimal multi-
// hop demo: a Producer feeds Router0 → Router1 → Router2 → Consumer,
// each link a separate credit_channel. Validates that credit-based
// backpressure composes correctly through multiple stages — when the
// consumer slows, each router's buffer fills and the producer
// back-pressures naturally. A mesh is just this pattern in 2D plus
// XY routing.
module FlitProducer (
  input logic clk,
  input logic rst,
  input logic [7:0] gen_pressure,
  output logic out_flits_send_valid,
  output logic [31:0] out_flits_send_data,
  input logic out_flits_credit_return
);

  logic [31:0] seq_no;
  logic [7:0] lfsr;
  always_comb begin
    if (1'd1) begin
      out_flits_send_valid = 1'd0;
      out_flits_send_data = 0;
    end
    if (__out_flits_can_send && lfsr < gen_pressure) begin
      if (1'd1) begin
        out_flits_send_valid = 1'd1;
        out_flits_send_data = seq_no;
      end
    end
  end
  always_ff @(posedge clk) begin
    if (rst) begin
      lfsr <= 8'd90;
      seq_no <= 32'd0;
    end else begin
      if (lfsr[0]) begin
        lfsr <= lfsr >> 1 ^ 8'd184;
      end else begin
        lfsr <= lfsr >> 1;
      end
      if (__out_flits_can_send && lfsr < gen_pressure) begin
        seq_no <= 32'(seq_no + 32'd1);
      end
    end
  end
  
  // Auto-generated credit_channel state (PR #3b-ii, sender side)
  logic [$clog2((4) + 1) - 1:0] __out_flits_credit;
  wire  __out_flits_can_send = __out_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_flits_credit <= 4;
    end else begin
      if (out_flits_send_valid && !out_flits_credit_return) __out_flits_credit <= __out_flits_credit - 1;
      else if (out_flits_credit_return && !out_flits_send_valid) __out_flits_credit <= __out_flits_credit + 1;
    end
  end
  
  // synopsys translate_off
  // Auto-generated credit_channel protocol assertions (Tier 2)
  _auto_cc_out_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): FlitProducer._auto_cc_out_flits_credit_bounds");
  _auto_cc_out_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_flits_send_valid |-> __out_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): FlitProducer._auto_cc_out_flits_send_requires_credit");
  // synopsys translate_on

endmodule

// Forwarding router: pop a flit from the input as soon as it's
// valid AND the output has credit. Single-cycle forwarding — no
// internal buffering beyond what the credit_channel already provides.
module FlitRouter (
  input logic clk,
  input logic rst,
  input logic up_flits_send_valid,
  input logic [31:0] up_flits_send_data,
  output logic up_flits_credit_return,
  output logic down_flits_send_valid,
  output logic [31:0] down_flits_send_data,
  input logic down_flits_credit_return
);

  // upstream  (incoming)
  // downstream (outgoing)
  always_comb begin
    if (1'd1) begin
      down_flits_send_valid = 1'd0;
      down_flits_send_data = 0;
    end
    up_flits_credit_return = 1'd0;
    if (__up_flits_valid && __down_flits_can_send) begin
      if (1'd1) begin
        down_flits_send_valid = 1'd1;
        down_flits_send_data = __up_flits_data;
      end
      up_flits_credit_return = 1'd1;
    end
  end
  
  // Auto-generated credit_channel state (PR #3b-ii, sender side)
  logic [$clog2((4) + 1) - 1:0] __down_flits_credit;
  wire  __down_flits_can_send = __down_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __down_flits_credit <= 4;
    end else begin
      if (down_flits_send_valid && !down_flits_credit_return) __down_flits_credit <= __down_flits_credit - 1;
      else if (down_flits_credit_return && !down_flits_send_valid) __down_flits_credit <= __down_flits_credit + 1;
    end
  end
  
  // Auto-generated credit_channel target-side FIFO (PR #3b-iii)
  logic [(32) - 1:0] __up_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __up_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __up_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __up_flits_occ;
  wire  __up_flits_valid = __up_flits_occ != 0;
  wire [(32) - 1:0] __up_flits_data = __up_flits_buf[__up_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __up_flits_head <= 0;
      __up_flits_tail <= 0;
      __up_flits_occ  <= 0;
    end else begin
      if (up_flits_send_valid) begin
        __up_flits_buf[__up_flits_tail] <= up_flits_send_data;
        __up_flits_tail <= (__up_flits_tail + 1) % (4);
      end
      if ((up_flits_credit_return && __up_flits_valid)) __up_flits_head <= (__up_flits_head + 1) % (4);
      if (up_flits_send_valid && !(up_flits_credit_return && __up_flits_valid)) __up_flits_occ <= __up_flits_occ + 1;
      else if (!up_flits_send_valid &&  (up_flits_credit_return && __up_flits_valid)) __up_flits_occ <= __up_flits_occ - 1;
    end
  end
  
  // synopsys translate_off
  // Auto-generated credit_channel protocol assertions (Tier 2)
  _auto_cc_down_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __down_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): FlitRouter._auto_cc_down_flits_credit_bounds");
  _auto_cc_down_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) down_flits_send_valid |-> __down_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): FlitRouter._auto_cc_down_flits_send_requires_credit");
  _auto_cc_up_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) up_flits_credit_return |-> __up_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): FlitRouter._auto_cc_up_flits_credit_return_requires_buffered");
  // synopsys translate_on

endmodule

module FlitConsumer (
  input logic clk,
  input logic rst,
  input logic [7:0] pop_pressure,
  input logic incoming_flits_send_valid,
  input logic [31:0] incoming_flits_send_data,
  output logic incoming_flits_credit_return,
  output logic [31:0] popped_count,
  output logic [31:0] last_seq,
  output logic in_order,
  output logic saw_any
);

  logic [7:0] lfsr;
  always_comb begin
    incoming_flits_credit_return = 1'd0;
    if (__incoming_flits_valid && lfsr < pop_pressure) begin
      incoming_flits_credit_return = 1'd1;
    end
  end
  always_ff @(posedge clk) begin
    if (rst) begin
      in_order <= 1'd1;
      last_seq <= 32'd0;
      lfsr <= 8'd195;
      popped_count <= 32'd0;
      saw_any <= 1'd0;
    end else begin
      if (lfsr[0]) begin
        lfsr <= lfsr >> 1 ^ 8'd184;
      end else begin
        lfsr <= lfsr >> 1;
      end
      if (__incoming_flits_valid && lfsr < pop_pressure) begin
        popped_count <= 32'(popped_count + 32'd1);
        last_seq <= __incoming_flits_data;
        if (saw_any && __incoming_flits_data != 32'(last_seq + 32'd1)) begin
          in_order <= 1'd0;
        end
        saw_any <= 1'd1;
      end
    end
  end
  
  // Auto-generated credit_channel target-side FIFO (PR #3b-iii)
  logic [(32) - 1:0] __incoming_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __incoming_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __incoming_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __incoming_flits_occ;
  wire  __incoming_flits_valid = __incoming_flits_occ != 0;
  wire [(32) - 1:0] __incoming_flits_data = __incoming_flits_buf[__incoming_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __incoming_flits_head <= 0;
      __incoming_flits_tail <= 0;
      __incoming_flits_occ  <= 0;
    end else begin
      if (incoming_flits_send_valid) begin
        __incoming_flits_buf[__incoming_flits_tail] <= incoming_flits_send_data;
        __incoming_flits_tail <= (__incoming_flits_tail + 1) % (4);
      end
      if ((incoming_flits_credit_return && __incoming_flits_valid)) __incoming_flits_head <= (__incoming_flits_head + 1) % (4);
      if (incoming_flits_send_valid && !(incoming_flits_credit_return && __incoming_flits_valid)) __incoming_flits_occ <= __incoming_flits_occ + 1;
      else if (!incoming_flits_send_valid &&  (incoming_flits_credit_return && __incoming_flits_valid)) __incoming_flits_occ <= __incoming_flits_occ - 1;
    end
  end
  
  // synopsys translate_off
  // Auto-generated credit_channel protocol assertions (Tier 2)
  _auto_cc_incoming_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) incoming_flits_credit_return |-> __incoming_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): FlitConsumer._auto_cc_incoming_flits_credit_return_requires_buffered");
  // synopsys translate_on

endmodule

module NocChainTop (
  input logic clk,
  input logic rst,
  input logic [7:0] gen_pressure,
  input logic [7:0] pop_pressure,
  output logic [31:0] popped_count,
  output logic [31:0] last_seq,
  output logic in_order,
  output logic saw_any
);

  logic link_p_r0_flits_send_valid;
  logic [31:0] link_p_r0_flits_send_data;
  logic link_p_r0_flits_credit_return;
  FlitProducer prod (
    .clk(clk),
    .rst(rst),
    .gen_pressure(gen_pressure),
    .out_flits_send_valid(link_p_r0_flits_send_valid),
    .out_flits_send_data(link_p_r0_flits_send_data),
    .out_flits_credit_return(link_p_r0_flits_credit_return)
  );
  logic link_r0_r1_flits_send_valid;
  logic [31:0] link_r0_r1_flits_send_data;
  logic link_r0_r1_flits_credit_return;
  FlitRouter r0 (
    .clk(clk),
    .rst(rst),
    .up_flits_send_valid(link_p_r0_flits_send_valid),
    .up_flits_send_data(link_p_r0_flits_send_data),
    .up_flits_credit_return(link_p_r0_flits_credit_return),
    .down_flits_send_valid(link_r0_r1_flits_send_valid),
    .down_flits_send_data(link_r0_r1_flits_send_data),
    .down_flits_credit_return(link_r0_r1_flits_credit_return)
  );
  logic link_r1_r2_flits_send_valid;
  logic [31:0] link_r1_r2_flits_send_data;
  logic link_r1_r2_flits_credit_return;
  FlitRouter r1 (
    .clk(clk),
    .rst(rst),
    .up_flits_send_valid(link_r0_r1_flits_send_valid),
    .up_flits_send_data(link_r0_r1_flits_send_data),
    .up_flits_credit_return(link_r0_r1_flits_credit_return),
    .down_flits_send_valid(link_r1_r2_flits_send_valid),
    .down_flits_send_data(link_r1_r2_flits_send_data),
    .down_flits_credit_return(link_r1_r2_flits_credit_return)
  );
  logic link_r2_c_flits_send_valid;
  logic [31:0] link_r2_c_flits_send_data;
  logic link_r2_c_flits_credit_return;
  FlitRouter r2 (
    .clk(clk),
    .rst(rst),
    .up_flits_send_valid(link_r1_r2_flits_send_valid),
    .up_flits_send_data(link_r1_r2_flits_send_data),
    .up_flits_credit_return(link_r1_r2_flits_credit_return),
    .down_flits_send_valid(link_r2_c_flits_send_valid),
    .down_flits_send_data(link_r2_c_flits_send_data),
    .down_flits_credit_return(link_r2_c_flits_credit_return)
  );
  FlitConsumer cons (
    .clk(clk),
    .rst(rst),
    .pop_pressure(pop_pressure),
    .incoming_flits_send_valid(link_r2_c_flits_send_valid),
    .incoming_flits_send_data(link_r2_c_flits_send_data),
    .incoming_flits_credit_return(link_r2_c_flits_credit_return),
    .popped_count(popped_count),
    .last_seq(last_seq),
    .in_order(in_order),
    .saw_any(saw_any)
  );

endmodule

