// 4×4 mesh NoC router. Single VC, single-flit packets, XY routing,
// priority arbitration per output.
//
// Flit format (UInt<32>):
//   bits[1:0] = dest_x (4-router mesh: 0..3)
//   bits[3:2] = dest_y
//   bits[31:4] = 28-bit payload (we stuff seq_no there)
//
// XY routing: route X first (E or W until x-aligned), then Y (N or S),
// then deliver locally. A flit therefore turns at most once on its path,
// guaranteeing no deadlock.
//
// Per-output arbitration: priority order local > N > S > E > W.
module Router__X_0_Y_0 #(
  parameter int X = 0,
  parameter int Y = 0
) (
  input logic clk,
  input logic rst,
  input logic in_local_flits_send_valid,
  input logic [31:0] in_local_flits_send_data,
  output logic in_local_flits_credit_return,
  input logic in_n_flits_send_valid,
  input logic [31:0] in_n_flits_send_data,
  output logic in_n_flits_credit_return,
  input logic in_s_flits_send_valid,
  input logic [31:0] in_s_flits_send_data,
  output logic in_s_flits_credit_return,
  input logic in_e_flits_send_valid,
  input logic [31:0] in_e_flits_send_data,
  output logic in_e_flits_credit_return,
  input logic in_w_flits_send_valid,
  input logic [31:0] in_w_flits_send_data,
  output logic in_w_flits_credit_return,
  output logic out_local_flits_send_valid,
  output logic [31:0] out_local_flits_send_data,
  input logic out_local_flits_credit_return,
  output logic out_n_flits_send_valid,
  output logic [31:0] out_n_flits_send_data,
  input logic out_n_flits_credit_return,
  output logic out_s_flits_send_valid,
  output logic [31:0] out_s_flits_send_data,
  input logic out_s_flits_credit_return,
  output logic out_e_flits_send_valid,
  output logic [31:0] out_e_flits_send_data,
  input logic out_e_flits_credit_return,
  output logic out_w_flits_send_valid,
  output logic [31:0] out_w_flits_send_data,
  input logic out_w_flits_credit_return
);

  // ── Decode dest from each input's flit and pick the routed output.
  //   Output codes: 0=local, 1=N, 2=S, 3=E, 4=W.
  //   When no flit is valid we don't care; comb defaults to 0 below.
  logic [31:0] lx_data;
  assign lx_data = __in_local_flits_data;
  logic [31:0] nx_data;
  assign nx_data = __in_n_flits_data;
  logic [31:0] sx_data;
  assign sx_data = __in_s_flits_data;
  logic [31:0] ex_data;
  assign ex_data = __in_e_flits_data;
  logic [31:0] wx_data;
  assign wx_data = __in_w_flits_data;
  logic [1:0] lx_dx;
  assign lx_dx = lx_data[1:0];
  logic [1:0] lx_dy;
  assign lx_dy = lx_data[3:2];
  logic [1:0] nx_dx;
  assign nx_dx = nx_data[1:0];
  logic [1:0] nx_dy;
  assign nx_dy = nx_data[3:2];
  logic [1:0] sx_dx;
  assign sx_dx = sx_data[1:0];
  logic [1:0] sx_dy;
  assign sx_dy = sx_data[3:2];
  logic [1:0] ex_dx;
  assign ex_dx = ex_data[1:0];
  logic [1:0] ex_dy;
  assign ex_dy = ex_data[3:2];
  logic [1:0] wx_dx;
  assign wx_dx = wx_data[1:0];
  logic [1:0] wx_dy;
  assign wx_dy = wx_data[3:2];
  // Compile-time params as UInt<2> for comparison.
  logic [1:0] mx;
  assign mx = X;
  logic [1:0] my;
  assign my = Y;
  // For each input: which output do we want? (XY routing)
  logic [2:0] route_local;
  assign route_local = lx_dx > mx ? 3'd3 : lx_dx < mx ? 3'd4 : lx_dy > my ? 3'd1 : lx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_n;
  assign route_n = nx_dx > mx ? 3'd3 : nx_dx < mx ? 3'd4 : nx_dy > my ? 3'd1 : nx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_s;
  assign route_s = sx_dx > mx ? 3'd3 : sx_dx < mx ? 3'd4 : sx_dy > my ? 3'd1 : sx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_e;
  assign route_e = ex_dx > mx ? 3'd3 : ex_dx < mx ? 3'd4 : ex_dy > my ? 3'd1 : ex_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_w;
  assign route_w = wx_dx > mx ? 3'd3 : wx_dx < mx ? 3'd4 : wx_dy > my ? 3'd1 : wx_dy < my ? 3'd2 : 3'd0;
  // Want-bits: input I has valid AND wants output O.
  logic want_local_local;
  assign want_local_local = __in_local_flits_valid && route_local == 3'd0;
  logic want_n_local;
  assign want_n_local = __in_n_flits_valid && route_n == 3'd0;
  logic want_s_local;
  assign want_s_local = __in_s_flits_valid && route_s == 3'd0;
  logic want_e_local;
  assign want_e_local = __in_e_flits_valid && route_e == 3'd0;
  logic want_w_local;
  assign want_w_local = __in_w_flits_valid && route_w == 3'd0;
  logic want_local_n;
  assign want_local_n = __in_local_flits_valid && route_local == 3'd1;
  logic want_n_n;
  assign want_n_n = __in_n_flits_valid && route_n == 3'd1;
  logic want_s_n;
  assign want_s_n = __in_s_flits_valid && route_s == 3'd1;
  logic want_e_n;
  assign want_e_n = __in_e_flits_valid && route_e == 3'd1;
  logic want_w_n;
  assign want_w_n = __in_w_flits_valid && route_w == 3'd1;
  logic want_local_s;
  assign want_local_s = __in_local_flits_valid && route_local == 3'd2;
  logic want_n_s;
  assign want_n_s = __in_n_flits_valid && route_n == 3'd2;
  logic want_s_s;
  assign want_s_s = __in_s_flits_valid && route_s == 3'd2;
  logic want_e_s;
  assign want_e_s = __in_e_flits_valid && route_e == 3'd2;
  logic want_w_s;
  assign want_w_s = __in_w_flits_valid && route_w == 3'd2;
  logic want_local_e;
  assign want_local_e = __in_local_flits_valid && route_local == 3'd3;
  logic want_n_e;
  assign want_n_e = __in_n_flits_valid && route_n == 3'd3;
  logic want_s_e;
  assign want_s_e = __in_s_flits_valid && route_s == 3'd3;
  logic want_e_e;
  assign want_e_e = __in_e_flits_valid && route_e == 3'd3;
  logic want_w_e;
  assign want_w_e = __in_w_flits_valid && route_w == 3'd3;
  logic want_local_w;
  assign want_local_w = __in_local_flits_valid && route_local == 3'd4;
  logic want_n_w;
  assign want_n_w = __in_n_flits_valid && route_n == 3'd4;
  logic want_s_w;
  assign want_s_w = __in_s_flits_valid && route_s == 3'd4;
  logic want_e_w;
  assign want_e_w = __in_e_flits_valid && route_e == 3'd4;
  logic want_w_w;
  assign want_w_w = __in_w_flits_valid && route_w == 3'd4;
  // Per-output: pick winning input (priority local > N > S > E > W) AND
  // require can_send on that output. Pick code: 0=none, 1=local, 2=N,
  // 3=S, 4=E, 5=W.
  logic [2:0] pick_local;
  assign pick_local = __out_local_flits_can_send && want_local_local ? 3'd1 : __out_local_flits_can_send && want_n_local ? 3'd2 : __out_local_flits_can_send && want_s_local ? 3'd3 : __out_local_flits_can_send && want_e_local ? 3'd4 : __out_local_flits_can_send && want_w_local ? 3'd5 : 3'd0;
  logic [2:0] pick_n;
  assign pick_n = __out_n_flits_can_send && want_local_n ? 3'd1 : __out_n_flits_can_send && want_n_n ? 3'd2 : __out_n_flits_can_send && want_s_n ? 3'd3 : __out_n_flits_can_send && want_e_n ? 3'd4 : __out_n_flits_can_send && want_w_n ? 3'd5 : 3'd0;
  logic [2:0] pick_s;
  assign pick_s = __out_s_flits_can_send && want_local_s ? 3'd1 : __out_s_flits_can_send && want_n_s ? 3'd2 : __out_s_flits_can_send && want_s_s ? 3'd3 : __out_s_flits_can_send && want_e_s ? 3'd4 : __out_s_flits_can_send && want_w_s ? 3'd5 : 3'd0;
  logic [2:0] pick_e;
  assign pick_e = __out_e_flits_can_send && want_local_e ? 3'd1 : __out_e_flits_can_send && want_n_e ? 3'd2 : __out_e_flits_can_send && want_s_e ? 3'd3 : __out_e_flits_can_send && want_e_e ? 3'd4 : __out_e_flits_can_send && want_w_e ? 3'd5 : 3'd0;
  logic [2:0] pick_w;
  assign pick_w = __out_w_flits_can_send && want_local_w ? 3'd1 : __out_w_flits_can_send && want_n_w ? 3'd2 : __out_w_flits_can_send && want_s_w ? 3'd3 : __out_w_flits_can_send && want_e_w ? 3'd4 : __out_w_flits_can_send && want_w_w ? 3'd5 : 3'd0;
  // An input is "served" iff some output picked it.
  logic served_local;
  assign served_local = pick_local == 3'd1 || pick_n == 3'd1 || pick_s == 3'd1 || pick_e == 3'd1 || pick_w == 3'd1;
  logic served_n;
  assign served_n = pick_local == 3'd2 || pick_n == 3'd2 || pick_s == 3'd2 || pick_e == 3'd2 || pick_w == 3'd2;
  logic served_s;
  assign served_s = pick_local == 3'd3 || pick_n == 3'd3 || pick_s == 3'd3 || pick_e == 3'd3 || pick_w == 3'd3;
  logic served_e;
  assign served_e = pick_local == 3'd4 || pick_n == 3'd4 || pick_s == 3'd4 || pick_e == 3'd4 || pick_w == 3'd4;
  logic served_w;
  assign served_w = pick_local == 3'd5 || pick_n == 3'd5 || pick_s == 3'd5 || pick_e == 3'd5 || pick_w == 3'd5;
  assign out_local_flits_send_valid = pick_local != 3'd0;
  assign out_local_flits_send_data = pick_local == 3'd1 ? lx_data : pick_local == 3'd2 ? nx_data : pick_local == 3'd3 ? sx_data : pick_local == 3'd4 ? ex_data : wx_data;
  assign out_n_flits_send_valid = pick_n != 3'd0;
  assign out_n_flits_send_data = pick_n == 3'd1 ? lx_data : pick_n == 3'd2 ? nx_data : pick_n == 3'd3 ? sx_data : pick_n == 3'd4 ? ex_data : wx_data;
  assign out_s_flits_send_valid = pick_s != 3'd0;
  assign out_s_flits_send_data = pick_s == 3'd1 ? lx_data : pick_s == 3'd2 ? nx_data : pick_s == 3'd3 ? sx_data : pick_s == 3'd4 ? ex_data : wx_data;
  assign out_e_flits_send_valid = pick_e != 3'd0;
  assign out_e_flits_send_data = pick_e == 3'd1 ? lx_data : pick_e == 3'd2 ? nx_data : pick_e == 3'd3 ? sx_data : pick_e == 3'd4 ? ex_data : wx_data;
  assign out_w_flits_send_valid = pick_w != 3'd0;
  assign out_w_flits_send_data = pick_w == 3'd1 ? lx_data : pick_w == 3'd2 ? nx_data : pick_w == 3'd3 ? sx_data : pick_w == 3'd4 ? ex_data : wx_data;
  assign in_local_flits_credit_return = served_local;
  assign in_n_flits_credit_return = served_n;
  assign in_s_flits_credit_return = served_s;
  assign in_e_flits_credit_return = served_e;
  assign in_w_flits_credit_return = served_w;
  
  // Auto-generated credit_channel state (PR #3b-ii, sender side)
  logic [$clog2((4) + 1) - 1:0] __out_local_flits_credit;
  wire  __out_local_flits_can_send = __out_local_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_local_flits_credit <= 4;
    end else begin
      if (out_local_flits_send_valid && !out_local_flits_credit_return) __out_local_flits_credit <= __out_local_flits_credit - 1;
      else if (out_local_flits_credit_return && !out_local_flits_send_valid) __out_local_flits_credit <= __out_local_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_n_flits_credit;
  wire  __out_n_flits_can_send = __out_n_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_n_flits_credit <= 4;
    end else begin
      if (out_n_flits_send_valid && !out_n_flits_credit_return) __out_n_flits_credit <= __out_n_flits_credit - 1;
      else if (out_n_flits_credit_return && !out_n_flits_send_valid) __out_n_flits_credit <= __out_n_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_s_flits_credit;
  wire  __out_s_flits_can_send = __out_s_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_s_flits_credit <= 4;
    end else begin
      if (out_s_flits_send_valid && !out_s_flits_credit_return) __out_s_flits_credit <= __out_s_flits_credit - 1;
      else if (out_s_flits_credit_return && !out_s_flits_send_valid) __out_s_flits_credit <= __out_s_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_e_flits_credit;
  wire  __out_e_flits_can_send = __out_e_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_e_flits_credit <= 4;
    end else begin
      if (out_e_flits_send_valid && !out_e_flits_credit_return) __out_e_flits_credit <= __out_e_flits_credit - 1;
      else if (out_e_flits_credit_return && !out_e_flits_send_valid) __out_e_flits_credit <= __out_e_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_w_flits_credit;
  wire  __out_w_flits_can_send = __out_w_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_w_flits_credit <= 4;
    end else begin
      if (out_w_flits_send_valid && !out_w_flits_credit_return) __out_w_flits_credit <= __out_w_flits_credit - 1;
      else if (out_w_flits_credit_return && !out_w_flits_send_valid) __out_w_flits_credit <= __out_w_flits_credit + 1;
    end
  end
  
  // Auto-generated credit_channel target-side FIFO (PR #3b-iii)
  logic [(32) - 1:0] __in_local_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_local_flits_occ;
  wire  __in_local_flits_valid = __in_local_flits_occ != 0;
  wire [(32) - 1:0] __in_local_flits_data = __in_local_flits_buf[__in_local_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_local_flits_head <= 0;
      __in_local_flits_tail <= 0;
      __in_local_flits_occ  <= 0;
    end else begin
      if (in_local_flits_send_valid) begin
        __in_local_flits_buf[__in_local_flits_tail] <= in_local_flits_send_data;
        __in_local_flits_tail <= (__in_local_flits_tail + 1) % (4);
      end
      if ((in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_head <= (__in_local_flits_head + 1) % (4);
      if (in_local_flits_send_valid && !(in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ + 1;
      else if (!in_local_flits_send_valid &&  (in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_n_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_n_flits_occ;
  wire  __in_n_flits_valid = __in_n_flits_occ != 0;
  wire [(32) - 1:0] __in_n_flits_data = __in_n_flits_buf[__in_n_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_n_flits_head <= 0;
      __in_n_flits_tail <= 0;
      __in_n_flits_occ  <= 0;
    end else begin
      if (in_n_flits_send_valid) begin
        __in_n_flits_buf[__in_n_flits_tail] <= in_n_flits_send_data;
        __in_n_flits_tail <= (__in_n_flits_tail + 1) % (4);
      end
      if ((in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_head <= (__in_n_flits_head + 1) % (4);
      if (in_n_flits_send_valid && !(in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ + 1;
      else if (!in_n_flits_send_valid &&  (in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_s_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_s_flits_occ;
  wire  __in_s_flits_valid = __in_s_flits_occ != 0;
  wire [(32) - 1:0] __in_s_flits_data = __in_s_flits_buf[__in_s_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_s_flits_head <= 0;
      __in_s_flits_tail <= 0;
      __in_s_flits_occ  <= 0;
    end else begin
      if (in_s_flits_send_valid) begin
        __in_s_flits_buf[__in_s_flits_tail] <= in_s_flits_send_data;
        __in_s_flits_tail <= (__in_s_flits_tail + 1) % (4);
      end
      if ((in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_head <= (__in_s_flits_head + 1) % (4);
      if (in_s_flits_send_valid && !(in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ + 1;
      else if (!in_s_flits_send_valid &&  (in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_e_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_e_flits_occ;
  wire  __in_e_flits_valid = __in_e_flits_occ != 0;
  wire [(32) - 1:0] __in_e_flits_data = __in_e_flits_buf[__in_e_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_e_flits_head <= 0;
      __in_e_flits_tail <= 0;
      __in_e_flits_occ  <= 0;
    end else begin
      if (in_e_flits_send_valid) begin
        __in_e_flits_buf[__in_e_flits_tail] <= in_e_flits_send_data;
        __in_e_flits_tail <= (__in_e_flits_tail + 1) % (4);
      end
      if ((in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_head <= (__in_e_flits_head + 1) % (4);
      if (in_e_flits_send_valid && !(in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ + 1;
      else if (!in_e_flits_send_valid &&  (in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_w_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_w_flits_occ;
  wire  __in_w_flits_valid = __in_w_flits_occ != 0;
  wire [(32) - 1:0] __in_w_flits_data = __in_w_flits_buf[__in_w_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_w_flits_head <= 0;
      __in_w_flits_tail <= 0;
      __in_w_flits_occ  <= 0;
    end else begin
      if (in_w_flits_send_valid) begin
        __in_w_flits_buf[__in_w_flits_tail] <= in_w_flits_send_data;
        __in_w_flits_tail <= (__in_w_flits_tail + 1) % (4);
      end
      if ((in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_head <= (__in_w_flits_head + 1) % (4);
      if (in_w_flits_send_valid && !(in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ + 1;
      else if (!in_w_flits_send_valid &&  (in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ - 1;
    end
  end
  
  // synopsys translate_off
  // Auto-generated credit_channel protocol assertions (Tier 2)
  _auto_cc_out_local_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_local_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_0_Y_0._auto_cc_out_local_flits_credit_bounds");
  _auto_cc_out_local_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_local_flits_send_valid |-> __out_local_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_0_Y_0._auto_cc_out_local_flits_send_requires_credit");
  _auto_cc_out_n_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_n_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_0_Y_0._auto_cc_out_n_flits_credit_bounds");
  _auto_cc_out_n_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_n_flits_send_valid |-> __out_n_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_0_Y_0._auto_cc_out_n_flits_send_requires_credit");
  _auto_cc_out_s_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_s_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_0_Y_0._auto_cc_out_s_flits_credit_bounds");
  _auto_cc_out_s_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_s_flits_send_valid |-> __out_s_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_0_Y_0._auto_cc_out_s_flits_send_requires_credit");
  _auto_cc_out_e_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_e_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_0_Y_0._auto_cc_out_e_flits_credit_bounds");
  _auto_cc_out_e_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_e_flits_send_valid |-> __out_e_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_0_Y_0._auto_cc_out_e_flits_send_requires_credit");
  _auto_cc_out_w_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_w_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_0_Y_0._auto_cc_out_w_flits_credit_bounds");
  _auto_cc_out_w_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_w_flits_send_valid |-> __out_w_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_0_Y_0._auto_cc_out_w_flits_send_requires_credit");
  _auto_cc_in_local_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_local_flits_credit_return |-> __in_local_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_0_Y_0._auto_cc_in_local_flits_credit_return_requires_buffered");
  _auto_cc_in_n_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_n_flits_credit_return |-> __in_n_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_0_Y_0._auto_cc_in_n_flits_credit_return_requires_buffered");
  _auto_cc_in_s_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_s_flits_credit_return |-> __in_s_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_0_Y_0._auto_cc_in_s_flits_credit_return_requires_buffered");
  _auto_cc_in_e_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_e_flits_credit_return |-> __in_e_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_0_Y_0._auto_cc_in_e_flits_credit_return_requires_buffered");
  _auto_cc_in_w_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_w_flits_credit_return |-> __in_w_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_0_Y_0._auto_cc_in_w_flits_credit_return_requires_buffered");
  // synopsys translate_on

endmodule

module Router__X_1_Y_0 #(
  parameter int X = 1,
  parameter int Y = 0
) (
  input logic clk,
  input logic rst,
  input logic in_local_flits_send_valid,
  input logic [31:0] in_local_flits_send_data,
  output logic in_local_flits_credit_return,
  input logic in_n_flits_send_valid,
  input logic [31:0] in_n_flits_send_data,
  output logic in_n_flits_credit_return,
  input logic in_s_flits_send_valid,
  input logic [31:0] in_s_flits_send_data,
  output logic in_s_flits_credit_return,
  input logic in_e_flits_send_valid,
  input logic [31:0] in_e_flits_send_data,
  output logic in_e_flits_credit_return,
  input logic in_w_flits_send_valid,
  input logic [31:0] in_w_flits_send_data,
  output logic in_w_flits_credit_return,
  output logic out_local_flits_send_valid,
  output logic [31:0] out_local_flits_send_data,
  input logic out_local_flits_credit_return,
  output logic out_n_flits_send_valid,
  output logic [31:0] out_n_flits_send_data,
  input logic out_n_flits_credit_return,
  output logic out_s_flits_send_valid,
  output logic [31:0] out_s_flits_send_data,
  input logic out_s_flits_credit_return,
  output logic out_e_flits_send_valid,
  output logic [31:0] out_e_flits_send_data,
  input logic out_e_flits_credit_return,
  output logic out_w_flits_send_valid,
  output logic [31:0] out_w_flits_send_data,
  input logic out_w_flits_credit_return
);

  logic [31:0] lx_data;
  assign lx_data = __in_local_flits_data;
  logic [31:0] nx_data;
  assign nx_data = __in_n_flits_data;
  logic [31:0] sx_data;
  assign sx_data = __in_s_flits_data;
  logic [31:0] ex_data;
  assign ex_data = __in_e_flits_data;
  logic [31:0] wx_data;
  assign wx_data = __in_w_flits_data;
  logic [1:0] lx_dx;
  assign lx_dx = lx_data[1:0];
  logic [1:0] lx_dy;
  assign lx_dy = lx_data[3:2];
  logic [1:0] nx_dx;
  assign nx_dx = nx_data[1:0];
  logic [1:0] nx_dy;
  assign nx_dy = nx_data[3:2];
  logic [1:0] sx_dx;
  assign sx_dx = sx_data[1:0];
  logic [1:0] sx_dy;
  assign sx_dy = sx_data[3:2];
  logic [1:0] ex_dx;
  assign ex_dx = ex_data[1:0];
  logic [1:0] ex_dy;
  assign ex_dy = ex_data[3:2];
  logic [1:0] wx_dx;
  assign wx_dx = wx_data[1:0];
  logic [1:0] wx_dy;
  assign wx_dy = wx_data[3:2];
  logic [1:0] mx;
  assign mx = X;
  logic [1:0] my;
  assign my = Y;
  logic [2:0] route_local;
  assign route_local = lx_dx > mx ? 3'd3 : lx_dx < mx ? 3'd4 : lx_dy > my ? 3'd1 : lx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_n;
  assign route_n = nx_dx > mx ? 3'd3 : nx_dx < mx ? 3'd4 : nx_dy > my ? 3'd1 : nx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_s;
  assign route_s = sx_dx > mx ? 3'd3 : sx_dx < mx ? 3'd4 : sx_dy > my ? 3'd1 : sx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_e;
  assign route_e = ex_dx > mx ? 3'd3 : ex_dx < mx ? 3'd4 : ex_dy > my ? 3'd1 : ex_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_w;
  assign route_w = wx_dx > mx ? 3'd3 : wx_dx < mx ? 3'd4 : wx_dy > my ? 3'd1 : wx_dy < my ? 3'd2 : 3'd0;
  logic want_local_local;
  assign want_local_local = __in_local_flits_valid && route_local == 3'd0;
  logic want_n_local;
  assign want_n_local = __in_n_flits_valid && route_n == 3'd0;
  logic want_s_local;
  assign want_s_local = __in_s_flits_valid && route_s == 3'd0;
  logic want_e_local;
  assign want_e_local = __in_e_flits_valid && route_e == 3'd0;
  logic want_w_local;
  assign want_w_local = __in_w_flits_valid && route_w == 3'd0;
  logic want_local_n;
  assign want_local_n = __in_local_flits_valid && route_local == 3'd1;
  logic want_n_n;
  assign want_n_n = __in_n_flits_valid && route_n == 3'd1;
  logic want_s_n;
  assign want_s_n = __in_s_flits_valid && route_s == 3'd1;
  logic want_e_n;
  assign want_e_n = __in_e_flits_valid && route_e == 3'd1;
  logic want_w_n;
  assign want_w_n = __in_w_flits_valid && route_w == 3'd1;
  logic want_local_s;
  assign want_local_s = __in_local_flits_valid && route_local == 3'd2;
  logic want_n_s;
  assign want_n_s = __in_n_flits_valid && route_n == 3'd2;
  logic want_s_s;
  assign want_s_s = __in_s_flits_valid && route_s == 3'd2;
  logic want_e_s;
  assign want_e_s = __in_e_flits_valid && route_e == 3'd2;
  logic want_w_s;
  assign want_w_s = __in_w_flits_valid && route_w == 3'd2;
  logic want_local_e;
  assign want_local_e = __in_local_flits_valid && route_local == 3'd3;
  logic want_n_e;
  assign want_n_e = __in_n_flits_valid && route_n == 3'd3;
  logic want_s_e;
  assign want_s_e = __in_s_flits_valid && route_s == 3'd3;
  logic want_e_e;
  assign want_e_e = __in_e_flits_valid && route_e == 3'd3;
  logic want_w_e;
  assign want_w_e = __in_w_flits_valid && route_w == 3'd3;
  logic want_local_w;
  assign want_local_w = __in_local_flits_valid && route_local == 3'd4;
  logic want_n_w;
  assign want_n_w = __in_n_flits_valid && route_n == 3'd4;
  logic want_s_w;
  assign want_s_w = __in_s_flits_valid && route_s == 3'd4;
  logic want_e_w;
  assign want_e_w = __in_e_flits_valid && route_e == 3'd4;
  logic want_w_w;
  assign want_w_w = __in_w_flits_valid && route_w == 3'd4;
  logic [2:0] pick_local;
  assign pick_local = __out_local_flits_can_send && want_local_local ? 3'd1 : __out_local_flits_can_send && want_n_local ? 3'd2 : __out_local_flits_can_send && want_s_local ? 3'd3 : __out_local_flits_can_send && want_e_local ? 3'd4 : __out_local_flits_can_send && want_w_local ? 3'd5 : 3'd0;
  logic [2:0] pick_n;
  assign pick_n = __out_n_flits_can_send && want_local_n ? 3'd1 : __out_n_flits_can_send && want_n_n ? 3'd2 : __out_n_flits_can_send && want_s_n ? 3'd3 : __out_n_flits_can_send && want_e_n ? 3'd4 : __out_n_flits_can_send && want_w_n ? 3'd5 : 3'd0;
  logic [2:0] pick_s;
  assign pick_s = __out_s_flits_can_send && want_local_s ? 3'd1 : __out_s_flits_can_send && want_n_s ? 3'd2 : __out_s_flits_can_send && want_s_s ? 3'd3 : __out_s_flits_can_send && want_e_s ? 3'd4 : __out_s_flits_can_send && want_w_s ? 3'd5 : 3'd0;
  logic [2:0] pick_e;
  assign pick_e = __out_e_flits_can_send && want_local_e ? 3'd1 : __out_e_flits_can_send && want_n_e ? 3'd2 : __out_e_flits_can_send && want_s_e ? 3'd3 : __out_e_flits_can_send && want_e_e ? 3'd4 : __out_e_flits_can_send && want_w_e ? 3'd5 : 3'd0;
  logic [2:0] pick_w;
  assign pick_w = __out_w_flits_can_send && want_local_w ? 3'd1 : __out_w_flits_can_send && want_n_w ? 3'd2 : __out_w_flits_can_send && want_s_w ? 3'd3 : __out_w_flits_can_send && want_e_w ? 3'd4 : __out_w_flits_can_send && want_w_w ? 3'd5 : 3'd0;
  logic served_local;
  assign served_local = pick_local == 3'd1 || pick_n == 3'd1 || pick_s == 3'd1 || pick_e == 3'd1 || pick_w == 3'd1;
  logic served_n;
  assign served_n = pick_local == 3'd2 || pick_n == 3'd2 || pick_s == 3'd2 || pick_e == 3'd2 || pick_w == 3'd2;
  logic served_s;
  assign served_s = pick_local == 3'd3 || pick_n == 3'd3 || pick_s == 3'd3 || pick_e == 3'd3 || pick_w == 3'd3;
  logic served_e;
  assign served_e = pick_local == 3'd4 || pick_n == 3'd4 || pick_s == 3'd4 || pick_e == 3'd4 || pick_w == 3'd4;
  logic served_w;
  assign served_w = pick_local == 3'd5 || pick_n == 3'd5 || pick_s == 3'd5 || pick_e == 3'd5 || pick_w == 3'd5;
  assign out_local_flits_send_valid = pick_local != 3'd0;
  assign out_local_flits_send_data = pick_local == 3'd1 ? lx_data : pick_local == 3'd2 ? nx_data : pick_local == 3'd3 ? sx_data : pick_local == 3'd4 ? ex_data : wx_data;
  assign out_n_flits_send_valid = pick_n != 3'd0;
  assign out_n_flits_send_data = pick_n == 3'd1 ? lx_data : pick_n == 3'd2 ? nx_data : pick_n == 3'd3 ? sx_data : pick_n == 3'd4 ? ex_data : wx_data;
  assign out_s_flits_send_valid = pick_s != 3'd0;
  assign out_s_flits_send_data = pick_s == 3'd1 ? lx_data : pick_s == 3'd2 ? nx_data : pick_s == 3'd3 ? sx_data : pick_s == 3'd4 ? ex_data : wx_data;
  assign out_e_flits_send_valid = pick_e != 3'd0;
  assign out_e_flits_send_data = pick_e == 3'd1 ? lx_data : pick_e == 3'd2 ? nx_data : pick_e == 3'd3 ? sx_data : pick_e == 3'd4 ? ex_data : wx_data;
  assign out_w_flits_send_valid = pick_w != 3'd0;
  assign out_w_flits_send_data = pick_w == 3'd1 ? lx_data : pick_w == 3'd2 ? nx_data : pick_w == 3'd3 ? sx_data : pick_w == 3'd4 ? ex_data : wx_data;
  assign in_local_flits_credit_return = served_local;
  assign in_n_flits_credit_return = served_n;
  assign in_s_flits_credit_return = served_s;
  assign in_e_flits_credit_return = served_e;
  assign in_w_flits_credit_return = served_w;
  
  // Auto-generated credit_channel state (PR #3b-ii, sender side)
  logic [$clog2((4) + 1) - 1:0] __out_local_flits_credit;
  wire  __out_local_flits_can_send = __out_local_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_local_flits_credit <= 4;
    end else begin
      if (out_local_flits_send_valid && !out_local_flits_credit_return) __out_local_flits_credit <= __out_local_flits_credit - 1;
      else if (out_local_flits_credit_return && !out_local_flits_send_valid) __out_local_flits_credit <= __out_local_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_n_flits_credit;
  wire  __out_n_flits_can_send = __out_n_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_n_flits_credit <= 4;
    end else begin
      if (out_n_flits_send_valid && !out_n_flits_credit_return) __out_n_flits_credit <= __out_n_flits_credit - 1;
      else if (out_n_flits_credit_return && !out_n_flits_send_valid) __out_n_flits_credit <= __out_n_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_s_flits_credit;
  wire  __out_s_flits_can_send = __out_s_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_s_flits_credit <= 4;
    end else begin
      if (out_s_flits_send_valid && !out_s_flits_credit_return) __out_s_flits_credit <= __out_s_flits_credit - 1;
      else if (out_s_flits_credit_return && !out_s_flits_send_valid) __out_s_flits_credit <= __out_s_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_e_flits_credit;
  wire  __out_e_flits_can_send = __out_e_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_e_flits_credit <= 4;
    end else begin
      if (out_e_flits_send_valid && !out_e_flits_credit_return) __out_e_flits_credit <= __out_e_flits_credit - 1;
      else if (out_e_flits_credit_return && !out_e_flits_send_valid) __out_e_flits_credit <= __out_e_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_w_flits_credit;
  wire  __out_w_flits_can_send = __out_w_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_w_flits_credit <= 4;
    end else begin
      if (out_w_flits_send_valid && !out_w_flits_credit_return) __out_w_flits_credit <= __out_w_flits_credit - 1;
      else if (out_w_flits_credit_return && !out_w_flits_send_valid) __out_w_flits_credit <= __out_w_flits_credit + 1;
    end
  end
  
  // Auto-generated credit_channel target-side FIFO (PR #3b-iii)
  logic [(32) - 1:0] __in_local_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_local_flits_occ;
  wire  __in_local_flits_valid = __in_local_flits_occ != 0;
  wire [(32) - 1:0] __in_local_flits_data = __in_local_flits_buf[__in_local_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_local_flits_head <= 0;
      __in_local_flits_tail <= 0;
      __in_local_flits_occ  <= 0;
    end else begin
      if (in_local_flits_send_valid) begin
        __in_local_flits_buf[__in_local_flits_tail] <= in_local_flits_send_data;
        __in_local_flits_tail <= (__in_local_flits_tail + 1) % (4);
      end
      if ((in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_head <= (__in_local_flits_head + 1) % (4);
      if (in_local_flits_send_valid && !(in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ + 1;
      else if (!in_local_flits_send_valid &&  (in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_n_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_n_flits_occ;
  wire  __in_n_flits_valid = __in_n_flits_occ != 0;
  wire [(32) - 1:0] __in_n_flits_data = __in_n_flits_buf[__in_n_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_n_flits_head <= 0;
      __in_n_flits_tail <= 0;
      __in_n_flits_occ  <= 0;
    end else begin
      if (in_n_flits_send_valid) begin
        __in_n_flits_buf[__in_n_flits_tail] <= in_n_flits_send_data;
        __in_n_flits_tail <= (__in_n_flits_tail + 1) % (4);
      end
      if ((in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_head <= (__in_n_flits_head + 1) % (4);
      if (in_n_flits_send_valid && !(in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ + 1;
      else if (!in_n_flits_send_valid &&  (in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_s_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_s_flits_occ;
  wire  __in_s_flits_valid = __in_s_flits_occ != 0;
  wire [(32) - 1:0] __in_s_flits_data = __in_s_flits_buf[__in_s_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_s_flits_head <= 0;
      __in_s_flits_tail <= 0;
      __in_s_flits_occ  <= 0;
    end else begin
      if (in_s_flits_send_valid) begin
        __in_s_flits_buf[__in_s_flits_tail] <= in_s_flits_send_data;
        __in_s_flits_tail <= (__in_s_flits_tail + 1) % (4);
      end
      if ((in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_head <= (__in_s_flits_head + 1) % (4);
      if (in_s_flits_send_valid && !(in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ + 1;
      else if (!in_s_flits_send_valid &&  (in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_e_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_e_flits_occ;
  wire  __in_e_flits_valid = __in_e_flits_occ != 0;
  wire [(32) - 1:0] __in_e_flits_data = __in_e_flits_buf[__in_e_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_e_flits_head <= 0;
      __in_e_flits_tail <= 0;
      __in_e_flits_occ  <= 0;
    end else begin
      if (in_e_flits_send_valid) begin
        __in_e_flits_buf[__in_e_flits_tail] <= in_e_flits_send_data;
        __in_e_flits_tail <= (__in_e_flits_tail + 1) % (4);
      end
      if ((in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_head <= (__in_e_flits_head + 1) % (4);
      if (in_e_flits_send_valid && !(in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ + 1;
      else if (!in_e_flits_send_valid &&  (in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_w_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_w_flits_occ;
  wire  __in_w_flits_valid = __in_w_flits_occ != 0;
  wire [(32) - 1:0] __in_w_flits_data = __in_w_flits_buf[__in_w_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_w_flits_head <= 0;
      __in_w_flits_tail <= 0;
      __in_w_flits_occ  <= 0;
    end else begin
      if (in_w_flits_send_valid) begin
        __in_w_flits_buf[__in_w_flits_tail] <= in_w_flits_send_data;
        __in_w_flits_tail <= (__in_w_flits_tail + 1) % (4);
      end
      if ((in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_head <= (__in_w_flits_head + 1) % (4);
      if (in_w_flits_send_valid && !(in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ + 1;
      else if (!in_w_flits_send_valid &&  (in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ - 1;
    end
  end
  
  // synopsys translate_off
  // Auto-generated credit_channel protocol assertions (Tier 2)
  _auto_cc_out_local_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_local_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_1_Y_0._auto_cc_out_local_flits_credit_bounds");
  _auto_cc_out_local_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_local_flits_send_valid |-> __out_local_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_1_Y_0._auto_cc_out_local_flits_send_requires_credit");
  _auto_cc_out_n_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_n_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_1_Y_0._auto_cc_out_n_flits_credit_bounds");
  _auto_cc_out_n_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_n_flits_send_valid |-> __out_n_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_1_Y_0._auto_cc_out_n_flits_send_requires_credit");
  _auto_cc_out_s_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_s_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_1_Y_0._auto_cc_out_s_flits_credit_bounds");
  _auto_cc_out_s_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_s_flits_send_valid |-> __out_s_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_1_Y_0._auto_cc_out_s_flits_send_requires_credit");
  _auto_cc_out_e_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_e_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_1_Y_0._auto_cc_out_e_flits_credit_bounds");
  _auto_cc_out_e_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_e_flits_send_valid |-> __out_e_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_1_Y_0._auto_cc_out_e_flits_send_requires_credit");
  _auto_cc_out_w_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_w_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_1_Y_0._auto_cc_out_w_flits_credit_bounds");
  _auto_cc_out_w_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_w_flits_send_valid |-> __out_w_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_1_Y_0._auto_cc_out_w_flits_send_requires_credit");
  _auto_cc_in_local_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_local_flits_credit_return |-> __in_local_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_1_Y_0._auto_cc_in_local_flits_credit_return_requires_buffered");
  _auto_cc_in_n_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_n_flits_credit_return |-> __in_n_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_1_Y_0._auto_cc_in_n_flits_credit_return_requires_buffered");
  _auto_cc_in_s_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_s_flits_credit_return |-> __in_s_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_1_Y_0._auto_cc_in_s_flits_credit_return_requires_buffered");
  _auto_cc_in_e_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_e_flits_credit_return |-> __in_e_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_1_Y_0._auto_cc_in_e_flits_credit_return_requires_buffered");
  _auto_cc_in_w_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_w_flits_credit_return |-> __in_w_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_1_Y_0._auto_cc_in_w_flits_credit_return_requires_buffered");
  // synopsys translate_on

endmodule

module Router__X_2_Y_0 #(
  parameter int X = 2,
  parameter int Y = 0
) (
  input logic clk,
  input logic rst,
  input logic in_local_flits_send_valid,
  input logic [31:0] in_local_flits_send_data,
  output logic in_local_flits_credit_return,
  input logic in_n_flits_send_valid,
  input logic [31:0] in_n_flits_send_data,
  output logic in_n_flits_credit_return,
  input logic in_s_flits_send_valid,
  input logic [31:0] in_s_flits_send_data,
  output logic in_s_flits_credit_return,
  input logic in_e_flits_send_valid,
  input logic [31:0] in_e_flits_send_data,
  output logic in_e_flits_credit_return,
  input logic in_w_flits_send_valid,
  input logic [31:0] in_w_flits_send_data,
  output logic in_w_flits_credit_return,
  output logic out_local_flits_send_valid,
  output logic [31:0] out_local_flits_send_data,
  input logic out_local_flits_credit_return,
  output logic out_n_flits_send_valid,
  output logic [31:0] out_n_flits_send_data,
  input logic out_n_flits_credit_return,
  output logic out_s_flits_send_valid,
  output logic [31:0] out_s_flits_send_data,
  input logic out_s_flits_credit_return,
  output logic out_e_flits_send_valid,
  output logic [31:0] out_e_flits_send_data,
  input logic out_e_flits_credit_return,
  output logic out_w_flits_send_valid,
  output logic [31:0] out_w_flits_send_data,
  input logic out_w_flits_credit_return
);

  logic [31:0] lx_data;
  assign lx_data = __in_local_flits_data;
  logic [31:0] nx_data;
  assign nx_data = __in_n_flits_data;
  logic [31:0] sx_data;
  assign sx_data = __in_s_flits_data;
  logic [31:0] ex_data;
  assign ex_data = __in_e_flits_data;
  logic [31:0] wx_data;
  assign wx_data = __in_w_flits_data;
  logic [1:0] lx_dx;
  assign lx_dx = lx_data[1:0];
  logic [1:0] lx_dy;
  assign lx_dy = lx_data[3:2];
  logic [1:0] nx_dx;
  assign nx_dx = nx_data[1:0];
  logic [1:0] nx_dy;
  assign nx_dy = nx_data[3:2];
  logic [1:0] sx_dx;
  assign sx_dx = sx_data[1:0];
  logic [1:0] sx_dy;
  assign sx_dy = sx_data[3:2];
  logic [1:0] ex_dx;
  assign ex_dx = ex_data[1:0];
  logic [1:0] ex_dy;
  assign ex_dy = ex_data[3:2];
  logic [1:0] wx_dx;
  assign wx_dx = wx_data[1:0];
  logic [1:0] wx_dy;
  assign wx_dy = wx_data[3:2];
  logic [1:0] mx;
  assign mx = X;
  logic [1:0] my;
  assign my = Y;
  logic [2:0] route_local;
  assign route_local = lx_dx > mx ? 3'd3 : lx_dx < mx ? 3'd4 : lx_dy > my ? 3'd1 : lx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_n;
  assign route_n = nx_dx > mx ? 3'd3 : nx_dx < mx ? 3'd4 : nx_dy > my ? 3'd1 : nx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_s;
  assign route_s = sx_dx > mx ? 3'd3 : sx_dx < mx ? 3'd4 : sx_dy > my ? 3'd1 : sx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_e;
  assign route_e = ex_dx > mx ? 3'd3 : ex_dx < mx ? 3'd4 : ex_dy > my ? 3'd1 : ex_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_w;
  assign route_w = wx_dx > mx ? 3'd3 : wx_dx < mx ? 3'd4 : wx_dy > my ? 3'd1 : wx_dy < my ? 3'd2 : 3'd0;
  logic want_local_local;
  assign want_local_local = __in_local_flits_valid && route_local == 3'd0;
  logic want_n_local;
  assign want_n_local = __in_n_flits_valid && route_n == 3'd0;
  logic want_s_local;
  assign want_s_local = __in_s_flits_valid && route_s == 3'd0;
  logic want_e_local;
  assign want_e_local = __in_e_flits_valid && route_e == 3'd0;
  logic want_w_local;
  assign want_w_local = __in_w_flits_valid && route_w == 3'd0;
  logic want_local_n;
  assign want_local_n = __in_local_flits_valid && route_local == 3'd1;
  logic want_n_n;
  assign want_n_n = __in_n_flits_valid && route_n == 3'd1;
  logic want_s_n;
  assign want_s_n = __in_s_flits_valid && route_s == 3'd1;
  logic want_e_n;
  assign want_e_n = __in_e_flits_valid && route_e == 3'd1;
  logic want_w_n;
  assign want_w_n = __in_w_flits_valid && route_w == 3'd1;
  logic want_local_s;
  assign want_local_s = __in_local_flits_valid && route_local == 3'd2;
  logic want_n_s;
  assign want_n_s = __in_n_flits_valid && route_n == 3'd2;
  logic want_s_s;
  assign want_s_s = __in_s_flits_valid && route_s == 3'd2;
  logic want_e_s;
  assign want_e_s = __in_e_flits_valid && route_e == 3'd2;
  logic want_w_s;
  assign want_w_s = __in_w_flits_valid && route_w == 3'd2;
  logic want_local_e;
  assign want_local_e = __in_local_flits_valid && route_local == 3'd3;
  logic want_n_e;
  assign want_n_e = __in_n_flits_valid && route_n == 3'd3;
  logic want_s_e;
  assign want_s_e = __in_s_flits_valid && route_s == 3'd3;
  logic want_e_e;
  assign want_e_e = __in_e_flits_valid && route_e == 3'd3;
  logic want_w_e;
  assign want_w_e = __in_w_flits_valid && route_w == 3'd3;
  logic want_local_w;
  assign want_local_w = __in_local_flits_valid && route_local == 3'd4;
  logic want_n_w;
  assign want_n_w = __in_n_flits_valid && route_n == 3'd4;
  logic want_s_w;
  assign want_s_w = __in_s_flits_valid && route_s == 3'd4;
  logic want_e_w;
  assign want_e_w = __in_e_flits_valid && route_e == 3'd4;
  logic want_w_w;
  assign want_w_w = __in_w_flits_valid && route_w == 3'd4;
  logic [2:0] pick_local;
  assign pick_local = __out_local_flits_can_send && want_local_local ? 3'd1 : __out_local_flits_can_send && want_n_local ? 3'd2 : __out_local_flits_can_send && want_s_local ? 3'd3 : __out_local_flits_can_send && want_e_local ? 3'd4 : __out_local_flits_can_send && want_w_local ? 3'd5 : 3'd0;
  logic [2:0] pick_n;
  assign pick_n = __out_n_flits_can_send && want_local_n ? 3'd1 : __out_n_flits_can_send && want_n_n ? 3'd2 : __out_n_flits_can_send && want_s_n ? 3'd3 : __out_n_flits_can_send && want_e_n ? 3'd4 : __out_n_flits_can_send && want_w_n ? 3'd5 : 3'd0;
  logic [2:0] pick_s;
  assign pick_s = __out_s_flits_can_send && want_local_s ? 3'd1 : __out_s_flits_can_send && want_n_s ? 3'd2 : __out_s_flits_can_send && want_s_s ? 3'd3 : __out_s_flits_can_send && want_e_s ? 3'd4 : __out_s_flits_can_send && want_w_s ? 3'd5 : 3'd0;
  logic [2:0] pick_e;
  assign pick_e = __out_e_flits_can_send && want_local_e ? 3'd1 : __out_e_flits_can_send && want_n_e ? 3'd2 : __out_e_flits_can_send && want_s_e ? 3'd3 : __out_e_flits_can_send && want_e_e ? 3'd4 : __out_e_flits_can_send && want_w_e ? 3'd5 : 3'd0;
  logic [2:0] pick_w;
  assign pick_w = __out_w_flits_can_send && want_local_w ? 3'd1 : __out_w_flits_can_send && want_n_w ? 3'd2 : __out_w_flits_can_send && want_s_w ? 3'd3 : __out_w_flits_can_send && want_e_w ? 3'd4 : __out_w_flits_can_send && want_w_w ? 3'd5 : 3'd0;
  logic served_local;
  assign served_local = pick_local == 3'd1 || pick_n == 3'd1 || pick_s == 3'd1 || pick_e == 3'd1 || pick_w == 3'd1;
  logic served_n;
  assign served_n = pick_local == 3'd2 || pick_n == 3'd2 || pick_s == 3'd2 || pick_e == 3'd2 || pick_w == 3'd2;
  logic served_s;
  assign served_s = pick_local == 3'd3 || pick_n == 3'd3 || pick_s == 3'd3 || pick_e == 3'd3 || pick_w == 3'd3;
  logic served_e;
  assign served_e = pick_local == 3'd4 || pick_n == 3'd4 || pick_s == 3'd4 || pick_e == 3'd4 || pick_w == 3'd4;
  logic served_w;
  assign served_w = pick_local == 3'd5 || pick_n == 3'd5 || pick_s == 3'd5 || pick_e == 3'd5 || pick_w == 3'd5;
  assign out_local_flits_send_valid = pick_local != 3'd0;
  assign out_local_flits_send_data = pick_local == 3'd1 ? lx_data : pick_local == 3'd2 ? nx_data : pick_local == 3'd3 ? sx_data : pick_local == 3'd4 ? ex_data : wx_data;
  assign out_n_flits_send_valid = pick_n != 3'd0;
  assign out_n_flits_send_data = pick_n == 3'd1 ? lx_data : pick_n == 3'd2 ? nx_data : pick_n == 3'd3 ? sx_data : pick_n == 3'd4 ? ex_data : wx_data;
  assign out_s_flits_send_valid = pick_s != 3'd0;
  assign out_s_flits_send_data = pick_s == 3'd1 ? lx_data : pick_s == 3'd2 ? nx_data : pick_s == 3'd3 ? sx_data : pick_s == 3'd4 ? ex_data : wx_data;
  assign out_e_flits_send_valid = pick_e != 3'd0;
  assign out_e_flits_send_data = pick_e == 3'd1 ? lx_data : pick_e == 3'd2 ? nx_data : pick_e == 3'd3 ? sx_data : pick_e == 3'd4 ? ex_data : wx_data;
  assign out_w_flits_send_valid = pick_w != 3'd0;
  assign out_w_flits_send_data = pick_w == 3'd1 ? lx_data : pick_w == 3'd2 ? nx_data : pick_w == 3'd3 ? sx_data : pick_w == 3'd4 ? ex_data : wx_data;
  assign in_local_flits_credit_return = served_local;
  assign in_n_flits_credit_return = served_n;
  assign in_s_flits_credit_return = served_s;
  assign in_e_flits_credit_return = served_e;
  assign in_w_flits_credit_return = served_w;
  
  // Auto-generated credit_channel state (PR #3b-ii, sender side)
  logic [$clog2((4) + 1) - 1:0] __out_local_flits_credit;
  wire  __out_local_flits_can_send = __out_local_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_local_flits_credit <= 4;
    end else begin
      if (out_local_flits_send_valid && !out_local_flits_credit_return) __out_local_flits_credit <= __out_local_flits_credit - 1;
      else if (out_local_flits_credit_return && !out_local_flits_send_valid) __out_local_flits_credit <= __out_local_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_n_flits_credit;
  wire  __out_n_flits_can_send = __out_n_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_n_flits_credit <= 4;
    end else begin
      if (out_n_flits_send_valid && !out_n_flits_credit_return) __out_n_flits_credit <= __out_n_flits_credit - 1;
      else if (out_n_flits_credit_return && !out_n_flits_send_valid) __out_n_flits_credit <= __out_n_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_s_flits_credit;
  wire  __out_s_flits_can_send = __out_s_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_s_flits_credit <= 4;
    end else begin
      if (out_s_flits_send_valid && !out_s_flits_credit_return) __out_s_flits_credit <= __out_s_flits_credit - 1;
      else if (out_s_flits_credit_return && !out_s_flits_send_valid) __out_s_flits_credit <= __out_s_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_e_flits_credit;
  wire  __out_e_flits_can_send = __out_e_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_e_flits_credit <= 4;
    end else begin
      if (out_e_flits_send_valid && !out_e_flits_credit_return) __out_e_flits_credit <= __out_e_flits_credit - 1;
      else if (out_e_flits_credit_return && !out_e_flits_send_valid) __out_e_flits_credit <= __out_e_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_w_flits_credit;
  wire  __out_w_flits_can_send = __out_w_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_w_flits_credit <= 4;
    end else begin
      if (out_w_flits_send_valid && !out_w_flits_credit_return) __out_w_flits_credit <= __out_w_flits_credit - 1;
      else if (out_w_flits_credit_return && !out_w_flits_send_valid) __out_w_flits_credit <= __out_w_flits_credit + 1;
    end
  end
  
  // Auto-generated credit_channel target-side FIFO (PR #3b-iii)
  logic [(32) - 1:0] __in_local_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_local_flits_occ;
  wire  __in_local_flits_valid = __in_local_flits_occ != 0;
  wire [(32) - 1:0] __in_local_flits_data = __in_local_flits_buf[__in_local_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_local_flits_head <= 0;
      __in_local_flits_tail <= 0;
      __in_local_flits_occ  <= 0;
    end else begin
      if (in_local_flits_send_valid) begin
        __in_local_flits_buf[__in_local_flits_tail] <= in_local_flits_send_data;
        __in_local_flits_tail <= (__in_local_flits_tail + 1) % (4);
      end
      if ((in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_head <= (__in_local_flits_head + 1) % (4);
      if (in_local_flits_send_valid && !(in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ + 1;
      else if (!in_local_flits_send_valid &&  (in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_n_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_n_flits_occ;
  wire  __in_n_flits_valid = __in_n_flits_occ != 0;
  wire [(32) - 1:0] __in_n_flits_data = __in_n_flits_buf[__in_n_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_n_flits_head <= 0;
      __in_n_flits_tail <= 0;
      __in_n_flits_occ  <= 0;
    end else begin
      if (in_n_flits_send_valid) begin
        __in_n_flits_buf[__in_n_flits_tail] <= in_n_flits_send_data;
        __in_n_flits_tail <= (__in_n_flits_tail + 1) % (4);
      end
      if ((in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_head <= (__in_n_flits_head + 1) % (4);
      if (in_n_flits_send_valid && !(in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ + 1;
      else if (!in_n_flits_send_valid &&  (in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_s_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_s_flits_occ;
  wire  __in_s_flits_valid = __in_s_flits_occ != 0;
  wire [(32) - 1:0] __in_s_flits_data = __in_s_flits_buf[__in_s_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_s_flits_head <= 0;
      __in_s_flits_tail <= 0;
      __in_s_flits_occ  <= 0;
    end else begin
      if (in_s_flits_send_valid) begin
        __in_s_flits_buf[__in_s_flits_tail] <= in_s_flits_send_data;
        __in_s_flits_tail <= (__in_s_flits_tail + 1) % (4);
      end
      if ((in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_head <= (__in_s_flits_head + 1) % (4);
      if (in_s_flits_send_valid && !(in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ + 1;
      else if (!in_s_flits_send_valid &&  (in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_e_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_e_flits_occ;
  wire  __in_e_flits_valid = __in_e_flits_occ != 0;
  wire [(32) - 1:0] __in_e_flits_data = __in_e_flits_buf[__in_e_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_e_flits_head <= 0;
      __in_e_flits_tail <= 0;
      __in_e_flits_occ  <= 0;
    end else begin
      if (in_e_flits_send_valid) begin
        __in_e_flits_buf[__in_e_flits_tail] <= in_e_flits_send_data;
        __in_e_flits_tail <= (__in_e_flits_tail + 1) % (4);
      end
      if ((in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_head <= (__in_e_flits_head + 1) % (4);
      if (in_e_flits_send_valid && !(in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ + 1;
      else if (!in_e_flits_send_valid &&  (in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_w_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_w_flits_occ;
  wire  __in_w_flits_valid = __in_w_flits_occ != 0;
  wire [(32) - 1:0] __in_w_flits_data = __in_w_flits_buf[__in_w_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_w_flits_head <= 0;
      __in_w_flits_tail <= 0;
      __in_w_flits_occ  <= 0;
    end else begin
      if (in_w_flits_send_valid) begin
        __in_w_flits_buf[__in_w_flits_tail] <= in_w_flits_send_data;
        __in_w_flits_tail <= (__in_w_flits_tail + 1) % (4);
      end
      if ((in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_head <= (__in_w_flits_head + 1) % (4);
      if (in_w_flits_send_valid && !(in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ + 1;
      else if (!in_w_flits_send_valid &&  (in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ - 1;
    end
  end
  
  // synopsys translate_off
  // Auto-generated credit_channel protocol assertions (Tier 2)
  _auto_cc_out_local_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_local_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_2_Y_0._auto_cc_out_local_flits_credit_bounds");
  _auto_cc_out_local_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_local_flits_send_valid |-> __out_local_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_2_Y_0._auto_cc_out_local_flits_send_requires_credit");
  _auto_cc_out_n_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_n_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_2_Y_0._auto_cc_out_n_flits_credit_bounds");
  _auto_cc_out_n_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_n_flits_send_valid |-> __out_n_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_2_Y_0._auto_cc_out_n_flits_send_requires_credit");
  _auto_cc_out_s_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_s_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_2_Y_0._auto_cc_out_s_flits_credit_bounds");
  _auto_cc_out_s_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_s_flits_send_valid |-> __out_s_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_2_Y_0._auto_cc_out_s_flits_send_requires_credit");
  _auto_cc_out_e_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_e_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_2_Y_0._auto_cc_out_e_flits_credit_bounds");
  _auto_cc_out_e_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_e_flits_send_valid |-> __out_e_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_2_Y_0._auto_cc_out_e_flits_send_requires_credit");
  _auto_cc_out_w_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_w_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_2_Y_0._auto_cc_out_w_flits_credit_bounds");
  _auto_cc_out_w_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_w_flits_send_valid |-> __out_w_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_2_Y_0._auto_cc_out_w_flits_send_requires_credit");
  _auto_cc_in_local_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_local_flits_credit_return |-> __in_local_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_2_Y_0._auto_cc_in_local_flits_credit_return_requires_buffered");
  _auto_cc_in_n_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_n_flits_credit_return |-> __in_n_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_2_Y_0._auto_cc_in_n_flits_credit_return_requires_buffered");
  _auto_cc_in_s_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_s_flits_credit_return |-> __in_s_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_2_Y_0._auto_cc_in_s_flits_credit_return_requires_buffered");
  _auto_cc_in_e_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_e_flits_credit_return |-> __in_e_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_2_Y_0._auto_cc_in_e_flits_credit_return_requires_buffered");
  _auto_cc_in_w_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_w_flits_credit_return |-> __in_w_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_2_Y_0._auto_cc_in_w_flits_credit_return_requires_buffered");
  // synopsys translate_on

endmodule

module Router__X_3_Y_0 #(
  parameter int X = 3,
  parameter int Y = 0
) (
  input logic clk,
  input logic rst,
  input logic in_local_flits_send_valid,
  input logic [31:0] in_local_flits_send_data,
  output logic in_local_flits_credit_return,
  input logic in_n_flits_send_valid,
  input logic [31:0] in_n_flits_send_data,
  output logic in_n_flits_credit_return,
  input logic in_s_flits_send_valid,
  input logic [31:0] in_s_flits_send_data,
  output logic in_s_flits_credit_return,
  input logic in_e_flits_send_valid,
  input logic [31:0] in_e_flits_send_data,
  output logic in_e_flits_credit_return,
  input logic in_w_flits_send_valid,
  input logic [31:0] in_w_flits_send_data,
  output logic in_w_flits_credit_return,
  output logic out_local_flits_send_valid,
  output logic [31:0] out_local_flits_send_data,
  input logic out_local_flits_credit_return,
  output logic out_n_flits_send_valid,
  output logic [31:0] out_n_flits_send_data,
  input logic out_n_flits_credit_return,
  output logic out_s_flits_send_valid,
  output logic [31:0] out_s_flits_send_data,
  input logic out_s_flits_credit_return,
  output logic out_e_flits_send_valid,
  output logic [31:0] out_e_flits_send_data,
  input logic out_e_flits_credit_return,
  output logic out_w_flits_send_valid,
  output logic [31:0] out_w_flits_send_data,
  input logic out_w_flits_credit_return
);

  logic [31:0] lx_data;
  assign lx_data = __in_local_flits_data;
  logic [31:0] nx_data;
  assign nx_data = __in_n_flits_data;
  logic [31:0] sx_data;
  assign sx_data = __in_s_flits_data;
  logic [31:0] ex_data;
  assign ex_data = __in_e_flits_data;
  logic [31:0] wx_data;
  assign wx_data = __in_w_flits_data;
  logic [1:0] lx_dx;
  assign lx_dx = lx_data[1:0];
  logic [1:0] lx_dy;
  assign lx_dy = lx_data[3:2];
  logic [1:0] nx_dx;
  assign nx_dx = nx_data[1:0];
  logic [1:0] nx_dy;
  assign nx_dy = nx_data[3:2];
  logic [1:0] sx_dx;
  assign sx_dx = sx_data[1:0];
  logic [1:0] sx_dy;
  assign sx_dy = sx_data[3:2];
  logic [1:0] ex_dx;
  assign ex_dx = ex_data[1:0];
  logic [1:0] ex_dy;
  assign ex_dy = ex_data[3:2];
  logic [1:0] wx_dx;
  assign wx_dx = wx_data[1:0];
  logic [1:0] wx_dy;
  assign wx_dy = wx_data[3:2];
  logic [1:0] mx;
  assign mx = X;
  logic [1:0] my;
  assign my = Y;
  logic [2:0] route_local;
  assign route_local = lx_dx > mx ? 3'd3 : lx_dx < mx ? 3'd4 : lx_dy > my ? 3'd1 : lx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_n;
  assign route_n = nx_dx > mx ? 3'd3 : nx_dx < mx ? 3'd4 : nx_dy > my ? 3'd1 : nx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_s;
  assign route_s = sx_dx > mx ? 3'd3 : sx_dx < mx ? 3'd4 : sx_dy > my ? 3'd1 : sx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_e;
  assign route_e = ex_dx > mx ? 3'd3 : ex_dx < mx ? 3'd4 : ex_dy > my ? 3'd1 : ex_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_w;
  assign route_w = wx_dx > mx ? 3'd3 : wx_dx < mx ? 3'd4 : wx_dy > my ? 3'd1 : wx_dy < my ? 3'd2 : 3'd0;
  logic want_local_local;
  assign want_local_local = __in_local_flits_valid && route_local == 3'd0;
  logic want_n_local;
  assign want_n_local = __in_n_flits_valid && route_n == 3'd0;
  logic want_s_local;
  assign want_s_local = __in_s_flits_valid && route_s == 3'd0;
  logic want_e_local;
  assign want_e_local = __in_e_flits_valid && route_e == 3'd0;
  logic want_w_local;
  assign want_w_local = __in_w_flits_valid && route_w == 3'd0;
  logic want_local_n;
  assign want_local_n = __in_local_flits_valid && route_local == 3'd1;
  logic want_n_n;
  assign want_n_n = __in_n_flits_valid && route_n == 3'd1;
  logic want_s_n;
  assign want_s_n = __in_s_flits_valid && route_s == 3'd1;
  logic want_e_n;
  assign want_e_n = __in_e_flits_valid && route_e == 3'd1;
  logic want_w_n;
  assign want_w_n = __in_w_flits_valid && route_w == 3'd1;
  logic want_local_s;
  assign want_local_s = __in_local_flits_valid && route_local == 3'd2;
  logic want_n_s;
  assign want_n_s = __in_n_flits_valid && route_n == 3'd2;
  logic want_s_s;
  assign want_s_s = __in_s_flits_valid && route_s == 3'd2;
  logic want_e_s;
  assign want_e_s = __in_e_flits_valid && route_e == 3'd2;
  logic want_w_s;
  assign want_w_s = __in_w_flits_valid && route_w == 3'd2;
  logic want_local_e;
  assign want_local_e = __in_local_flits_valid && route_local == 3'd3;
  logic want_n_e;
  assign want_n_e = __in_n_flits_valid && route_n == 3'd3;
  logic want_s_e;
  assign want_s_e = __in_s_flits_valid && route_s == 3'd3;
  logic want_e_e;
  assign want_e_e = __in_e_flits_valid && route_e == 3'd3;
  logic want_w_e;
  assign want_w_e = __in_w_flits_valid && route_w == 3'd3;
  logic want_local_w;
  assign want_local_w = __in_local_flits_valid && route_local == 3'd4;
  logic want_n_w;
  assign want_n_w = __in_n_flits_valid && route_n == 3'd4;
  logic want_s_w;
  assign want_s_w = __in_s_flits_valid && route_s == 3'd4;
  logic want_e_w;
  assign want_e_w = __in_e_flits_valid && route_e == 3'd4;
  logic want_w_w;
  assign want_w_w = __in_w_flits_valid && route_w == 3'd4;
  logic [2:0] pick_local;
  assign pick_local = __out_local_flits_can_send && want_local_local ? 3'd1 : __out_local_flits_can_send && want_n_local ? 3'd2 : __out_local_flits_can_send && want_s_local ? 3'd3 : __out_local_flits_can_send && want_e_local ? 3'd4 : __out_local_flits_can_send && want_w_local ? 3'd5 : 3'd0;
  logic [2:0] pick_n;
  assign pick_n = __out_n_flits_can_send && want_local_n ? 3'd1 : __out_n_flits_can_send && want_n_n ? 3'd2 : __out_n_flits_can_send && want_s_n ? 3'd3 : __out_n_flits_can_send && want_e_n ? 3'd4 : __out_n_flits_can_send && want_w_n ? 3'd5 : 3'd0;
  logic [2:0] pick_s;
  assign pick_s = __out_s_flits_can_send && want_local_s ? 3'd1 : __out_s_flits_can_send && want_n_s ? 3'd2 : __out_s_flits_can_send && want_s_s ? 3'd3 : __out_s_flits_can_send && want_e_s ? 3'd4 : __out_s_flits_can_send && want_w_s ? 3'd5 : 3'd0;
  logic [2:0] pick_e;
  assign pick_e = __out_e_flits_can_send && want_local_e ? 3'd1 : __out_e_flits_can_send && want_n_e ? 3'd2 : __out_e_flits_can_send && want_s_e ? 3'd3 : __out_e_flits_can_send && want_e_e ? 3'd4 : __out_e_flits_can_send && want_w_e ? 3'd5 : 3'd0;
  logic [2:0] pick_w;
  assign pick_w = __out_w_flits_can_send && want_local_w ? 3'd1 : __out_w_flits_can_send && want_n_w ? 3'd2 : __out_w_flits_can_send && want_s_w ? 3'd3 : __out_w_flits_can_send && want_e_w ? 3'd4 : __out_w_flits_can_send && want_w_w ? 3'd5 : 3'd0;
  logic served_local;
  assign served_local = pick_local == 3'd1 || pick_n == 3'd1 || pick_s == 3'd1 || pick_e == 3'd1 || pick_w == 3'd1;
  logic served_n;
  assign served_n = pick_local == 3'd2 || pick_n == 3'd2 || pick_s == 3'd2 || pick_e == 3'd2 || pick_w == 3'd2;
  logic served_s;
  assign served_s = pick_local == 3'd3 || pick_n == 3'd3 || pick_s == 3'd3 || pick_e == 3'd3 || pick_w == 3'd3;
  logic served_e;
  assign served_e = pick_local == 3'd4 || pick_n == 3'd4 || pick_s == 3'd4 || pick_e == 3'd4 || pick_w == 3'd4;
  logic served_w;
  assign served_w = pick_local == 3'd5 || pick_n == 3'd5 || pick_s == 3'd5 || pick_e == 3'd5 || pick_w == 3'd5;
  assign out_local_flits_send_valid = pick_local != 3'd0;
  assign out_local_flits_send_data = pick_local == 3'd1 ? lx_data : pick_local == 3'd2 ? nx_data : pick_local == 3'd3 ? sx_data : pick_local == 3'd4 ? ex_data : wx_data;
  assign out_n_flits_send_valid = pick_n != 3'd0;
  assign out_n_flits_send_data = pick_n == 3'd1 ? lx_data : pick_n == 3'd2 ? nx_data : pick_n == 3'd3 ? sx_data : pick_n == 3'd4 ? ex_data : wx_data;
  assign out_s_flits_send_valid = pick_s != 3'd0;
  assign out_s_flits_send_data = pick_s == 3'd1 ? lx_data : pick_s == 3'd2 ? nx_data : pick_s == 3'd3 ? sx_data : pick_s == 3'd4 ? ex_data : wx_data;
  assign out_e_flits_send_valid = pick_e != 3'd0;
  assign out_e_flits_send_data = pick_e == 3'd1 ? lx_data : pick_e == 3'd2 ? nx_data : pick_e == 3'd3 ? sx_data : pick_e == 3'd4 ? ex_data : wx_data;
  assign out_w_flits_send_valid = pick_w != 3'd0;
  assign out_w_flits_send_data = pick_w == 3'd1 ? lx_data : pick_w == 3'd2 ? nx_data : pick_w == 3'd3 ? sx_data : pick_w == 3'd4 ? ex_data : wx_data;
  assign in_local_flits_credit_return = served_local;
  assign in_n_flits_credit_return = served_n;
  assign in_s_flits_credit_return = served_s;
  assign in_e_flits_credit_return = served_e;
  assign in_w_flits_credit_return = served_w;
  
  // Auto-generated credit_channel state (PR #3b-ii, sender side)
  logic [$clog2((4) + 1) - 1:0] __out_local_flits_credit;
  wire  __out_local_flits_can_send = __out_local_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_local_flits_credit <= 4;
    end else begin
      if (out_local_flits_send_valid && !out_local_flits_credit_return) __out_local_flits_credit <= __out_local_flits_credit - 1;
      else if (out_local_flits_credit_return && !out_local_flits_send_valid) __out_local_flits_credit <= __out_local_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_n_flits_credit;
  wire  __out_n_flits_can_send = __out_n_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_n_flits_credit <= 4;
    end else begin
      if (out_n_flits_send_valid && !out_n_flits_credit_return) __out_n_flits_credit <= __out_n_flits_credit - 1;
      else if (out_n_flits_credit_return && !out_n_flits_send_valid) __out_n_flits_credit <= __out_n_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_s_flits_credit;
  wire  __out_s_flits_can_send = __out_s_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_s_flits_credit <= 4;
    end else begin
      if (out_s_flits_send_valid && !out_s_flits_credit_return) __out_s_flits_credit <= __out_s_flits_credit - 1;
      else if (out_s_flits_credit_return && !out_s_flits_send_valid) __out_s_flits_credit <= __out_s_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_e_flits_credit;
  wire  __out_e_flits_can_send = __out_e_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_e_flits_credit <= 4;
    end else begin
      if (out_e_flits_send_valid && !out_e_flits_credit_return) __out_e_flits_credit <= __out_e_flits_credit - 1;
      else if (out_e_flits_credit_return && !out_e_flits_send_valid) __out_e_flits_credit <= __out_e_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_w_flits_credit;
  wire  __out_w_flits_can_send = __out_w_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_w_flits_credit <= 4;
    end else begin
      if (out_w_flits_send_valid && !out_w_flits_credit_return) __out_w_flits_credit <= __out_w_flits_credit - 1;
      else if (out_w_flits_credit_return && !out_w_flits_send_valid) __out_w_flits_credit <= __out_w_flits_credit + 1;
    end
  end
  
  // Auto-generated credit_channel target-side FIFO (PR #3b-iii)
  logic [(32) - 1:0] __in_local_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_local_flits_occ;
  wire  __in_local_flits_valid = __in_local_flits_occ != 0;
  wire [(32) - 1:0] __in_local_flits_data = __in_local_flits_buf[__in_local_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_local_flits_head <= 0;
      __in_local_flits_tail <= 0;
      __in_local_flits_occ  <= 0;
    end else begin
      if (in_local_flits_send_valid) begin
        __in_local_flits_buf[__in_local_flits_tail] <= in_local_flits_send_data;
        __in_local_flits_tail <= (__in_local_flits_tail + 1) % (4);
      end
      if ((in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_head <= (__in_local_flits_head + 1) % (4);
      if (in_local_flits_send_valid && !(in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ + 1;
      else if (!in_local_flits_send_valid &&  (in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_n_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_n_flits_occ;
  wire  __in_n_flits_valid = __in_n_flits_occ != 0;
  wire [(32) - 1:0] __in_n_flits_data = __in_n_flits_buf[__in_n_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_n_flits_head <= 0;
      __in_n_flits_tail <= 0;
      __in_n_flits_occ  <= 0;
    end else begin
      if (in_n_flits_send_valid) begin
        __in_n_flits_buf[__in_n_flits_tail] <= in_n_flits_send_data;
        __in_n_flits_tail <= (__in_n_flits_tail + 1) % (4);
      end
      if ((in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_head <= (__in_n_flits_head + 1) % (4);
      if (in_n_flits_send_valid && !(in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ + 1;
      else if (!in_n_flits_send_valid &&  (in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_s_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_s_flits_occ;
  wire  __in_s_flits_valid = __in_s_flits_occ != 0;
  wire [(32) - 1:0] __in_s_flits_data = __in_s_flits_buf[__in_s_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_s_flits_head <= 0;
      __in_s_flits_tail <= 0;
      __in_s_flits_occ  <= 0;
    end else begin
      if (in_s_flits_send_valid) begin
        __in_s_flits_buf[__in_s_flits_tail] <= in_s_flits_send_data;
        __in_s_flits_tail <= (__in_s_flits_tail + 1) % (4);
      end
      if ((in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_head <= (__in_s_flits_head + 1) % (4);
      if (in_s_flits_send_valid && !(in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ + 1;
      else if (!in_s_flits_send_valid &&  (in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_e_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_e_flits_occ;
  wire  __in_e_flits_valid = __in_e_flits_occ != 0;
  wire [(32) - 1:0] __in_e_flits_data = __in_e_flits_buf[__in_e_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_e_flits_head <= 0;
      __in_e_flits_tail <= 0;
      __in_e_flits_occ  <= 0;
    end else begin
      if (in_e_flits_send_valid) begin
        __in_e_flits_buf[__in_e_flits_tail] <= in_e_flits_send_data;
        __in_e_flits_tail <= (__in_e_flits_tail + 1) % (4);
      end
      if ((in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_head <= (__in_e_flits_head + 1) % (4);
      if (in_e_flits_send_valid && !(in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ + 1;
      else if (!in_e_flits_send_valid &&  (in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_w_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_w_flits_occ;
  wire  __in_w_flits_valid = __in_w_flits_occ != 0;
  wire [(32) - 1:0] __in_w_flits_data = __in_w_flits_buf[__in_w_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_w_flits_head <= 0;
      __in_w_flits_tail <= 0;
      __in_w_flits_occ  <= 0;
    end else begin
      if (in_w_flits_send_valid) begin
        __in_w_flits_buf[__in_w_flits_tail] <= in_w_flits_send_data;
        __in_w_flits_tail <= (__in_w_flits_tail + 1) % (4);
      end
      if ((in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_head <= (__in_w_flits_head + 1) % (4);
      if (in_w_flits_send_valid && !(in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ + 1;
      else if (!in_w_flits_send_valid &&  (in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ - 1;
    end
  end
  
  // synopsys translate_off
  // Auto-generated credit_channel protocol assertions (Tier 2)
  _auto_cc_out_local_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_local_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_3_Y_0._auto_cc_out_local_flits_credit_bounds");
  _auto_cc_out_local_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_local_flits_send_valid |-> __out_local_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_3_Y_0._auto_cc_out_local_flits_send_requires_credit");
  _auto_cc_out_n_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_n_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_3_Y_0._auto_cc_out_n_flits_credit_bounds");
  _auto_cc_out_n_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_n_flits_send_valid |-> __out_n_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_3_Y_0._auto_cc_out_n_flits_send_requires_credit");
  _auto_cc_out_s_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_s_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_3_Y_0._auto_cc_out_s_flits_credit_bounds");
  _auto_cc_out_s_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_s_flits_send_valid |-> __out_s_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_3_Y_0._auto_cc_out_s_flits_send_requires_credit");
  _auto_cc_out_e_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_e_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_3_Y_0._auto_cc_out_e_flits_credit_bounds");
  _auto_cc_out_e_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_e_flits_send_valid |-> __out_e_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_3_Y_0._auto_cc_out_e_flits_send_requires_credit");
  _auto_cc_out_w_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_w_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_3_Y_0._auto_cc_out_w_flits_credit_bounds");
  _auto_cc_out_w_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_w_flits_send_valid |-> __out_w_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_3_Y_0._auto_cc_out_w_flits_send_requires_credit");
  _auto_cc_in_local_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_local_flits_credit_return |-> __in_local_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_3_Y_0._auto_cc_in_local_flits_credit_return_requires_buffered");
  _auto_cc_in_n_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_n_flits_credit_return |-> __in_n_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_3_Y_0._auto_cc_in_n_flits_credit_return_requires_buffered");
  _auto_cc_in_s_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_s_flits_credit_return |-> __in_s_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_3_Y_0._auto_cc_in_s_flits_credit_return_requires_buffered");
  _auto_cc_in_e_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_e_flits_credit_return |-> __in_e_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_3_Y_0._auto_cc_in_e_flits_credit_return_requires_buffered");
  _auto_cc_in_w_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_w_flits_credit_return |-> __in_w_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_3_Y_0._auto_cc_in_w_flits_credit_return_requires_buffered");
  // synopsys translate_on

endmodule

module Router__X_0_Y_1 #(
  parameter int X = 0,
  parameter int Y = 1
) (
  input logic clk,
  input logic rst,
  input logic in_local_flits_send_valid,
  input logic [31:0] in_local_flits_send_data,
  output logic in_local_flits_credit_return,
  input logic in_n_flits_send_valid,
  input logic [31:0] in_n_flits_send_data,
  output logic in_n_flits_credit_return,
  input logic in_s_flits_send_valid,
  input logic [31:0] in_s_flits_send_data,
  output logic in_s_flits_credit_return,
  input logic in_e_flits_send_valid,
  input logic [31:0] in_e_flits_send_data,
  output logic in_e_flits_credit_return,
  input logic in_w_flits_send_valid,
  input logic [31:0] in_w_flits_send_data,
  output logic in_w_flits_credit_return,
  output logic out_local_flits_send_valid,
  output logic [31:0] out_local_flits_send_data,
  input logic out_local_flits_credit_return,
  output logic out_n_flits_send_valid,
  output logic [31:0] out_n_flits_send_data,
  input logic out_n_flits_credit_return,
  output logic out_s_flits_send_valid,
  output logic [31:0] out_s_flits_send_data,
  input logic out_s_flits_credit_return,
  output logic out_e_flits_send_valid,
  output logic [31:0] out_e_flits_send_data,
  input logic out_e_flits_credit_return,
  output logic out_w_flits_send_valid,
  output logic [31:0] out_w_flits_send_data,
  input logic out_w_flits_credit_return
);

  logic [31:0] lx_data;
  assign lx_data = __in_local_flits_data;
  logic [31:0] nx_data;
  assign nx_data = __in_n_flits_data;
  logic [31:0] sx_data;
  assign sx_data = __in_s_flits_data;
  logic [31:0] ex_data;
  assign ex_data = __in_e_flits_data;
  logic [31:0] wx_data;
  assign wx_data = __in_w_flits_data;
  logic [1:0] lx_dx;
  assign lx_dx = lx_data[1:0];
  logic [1:0] lx_dy;
  assign lx_dy = lx_data[3:2];
  logic [1:0] nx_dx;
  assign nx_dx = nx_data[1:0];
  logic [1:0] nx_dy;
  assign nx_dy = nx_data[3:2];
  logic [1:0] sx_dx;
  assign sx_dx = sx_data[1:0];
  logic [1:0] sx_dy;
  assign sx_dy = sx_data[3:2];
  logic [1:0] ex_dx;
  assign ex_dx = ex_data[1:0];
  logic [1:0] ex_dy;
  assign ex_dy = ex_data[3:2];
  logic [1:0] wx_dx;
  assign wx_dx = wx_data[1:0];
  logic [1:0] wx_dy;
  assign wx_dy = wx_data[3:2];
  logic [1:0] mx;
  assign mx = X;
  logic [1:0] my;
  assign my = Y;
  logic [2:0] route_local;
  assign route_local = lx_dx > mx ? 3'd3 : lx_dx < mx ? 3'd4 : lx_dy > my ? 3'd1 : lx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_n;
  assign route_n = nx_dx > mx ? 3'd3 : nx_dx < mx ? 3'd4 : nx_dy > my ? 3'd1 : nx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_s;
  assign route_s = sx_dx > mx ? 3'd3 : sx_dx < mx ? 3'd4 : sx_dy > my ? 3'd1 : sx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_e;
  assign route_e = ex_dx > mx ? 3'd3 : ex_dx < mx ? 3'd4 : ex_dy > my ? 3'd1 : ex_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_w;
  assign route_w = wx_dx > mx ? 3'd3 : wx_dx < mx ? 3'd4 : wx_dy > my ? 3'd1 : wx_dy < my ? 3'd2 : 3'd0;
  logic want_local_local;
  assign want_local_local = __in_local_flits_valid && route_local == 3'd0;
  logic want_n_local;
  assign want_n_local = __in_n_flits_valid && route_n == 3'd0;
  logic want_s_local;
  assign want_s_local = __in_s_flits_valid && route_s == 3'd0;
  logic want_e_local;
  assign want_e_local = __in_e_flits_valid && route_e == 3'd0;
  logic want_w_local;
  assign want_w_local = __in_w_flits_valid && route_w == 3'd0;
  logic want_local_n;
  assign want_local_n = __in_local_flits_valid && route_local == 3'd1;
  logic want_n_n;
  assign want_n_n = __in_n_flits_valid && route_n == 3'd1;
  logic want_s_n;
  assign want_s_n = __in_s_flits_valid && route_s == 3'd1;
  logic want_e_n;
  assign want_e_n = __in_e_flits_valid && route_e == 3'd1;
  logic want_w_n;
  assign want_w_n = __in_w_flits_valid && route_w == 3'd1;
  logic want_local_s;
  assign want_local_s = __in_local_flits_valid && route_local == 3'd2;
  logic want_n_s;
  assign want_n_s = __in_n_flits_valid && route_n == 3'd2;
  logic want_s_s;
  assign want_s_s = __in_s_flits_valid && route_s == 3'd2;
  logic want_e_s;
  assign want_e_s = __in_e_flits_valid && route_e == 3'd2;
  logic want_w_s;
  assign want_w_s = __in_w_flits_valid && route_w == 3'd2;
  logic want_local_e;
  assign want_local_e = __in_local_flits_valid && route_local == 3'd3;
  logic want_n_e;
  assign want_n_e = __in_n_flits_valid && route_n == 3'd3;
  logic want_s_e;
  assign want_s_e = __in_s_flits_valid && route_s == 3'd3;
  logic want_e_e;
  assign want_e_e = __in_e_flits_valid && route_e == 3'd3;
  logic want_w_e;
  assign want_w_e = __in_w_flits_valid && route_w == 3'd3;
  logic want_local_w;
  assign want_local_w = __in_local_flits_valid && route_local == 3'd4;
  logic want_n_w;
  assign want_n_w = __in_n_flits_valid && route_n == 3'd4;
  logic want_s_w;
  assign want_s_w = __in_s_flits_valid && route_s == 3'd4;
  logic want_e_w;
  assign want_e_w = __in_e_flits_valid && route_e == 3'd4;
  logic want_w_w;
  assign want_w_w = __in_w_flits_valid && route_w == 3'd4;
  logic [2:0] pick_local;
  assign pick_local = __out_local_flits_can_send && want_local_local ? 3'd1 : __out_local_flits_can_send && want_n_local ? 3'd2 : __out_local_flits_can_send && want_s_local ? 3'd3 : __out_local_flits_can_send && want_e_local ? 3'd4 : __out_local_flits_can_send && want_w_local ? 3'd5 : 3'd0;
  logic [2:0] pick_n;
  assign pick_n = __out_n_flits_can_send && want_local_n ? 3'd1 : __out_n_flits_can_send && want_n_n ? 3'd2 : __out_n_flits_can_send && want_s_n ? 3'd3 : __out_n_flits_can_send && want_e_n ? 3'd4 : __out_n_flits_can_send && want_w_n ? 3'd5 : 3'd0;
  logic [2:0] pick_s;
  assign pick_s = __out_s_flits_can_send && want_local_s ? 3'd1 : __out_s_flits_can_send && want_n_s ? 3'd2 : __out_s_flits_can_send && want_s_s ? 3'd3 : __out_s_flits_can_send && want_e_s ? 3'd4 : __out_s_flits_can_send && want_w_s ? 3'd5 : 3'd0;
  logic [2:0] pick_e;
  assign pick_e = __out_e_flits_can_send && want_local_e ? 3'd1 : __out_e_flits_can_send && want_n_e ? 3'd2 : __out_e_flits_can_send && want_s_e ? 3'd3 : __out_e_flits_can_send && want_e_e ? 3'd4 : __out_e_flits_can_send && want_w_e ? 3'd5 : 3'd0;
  logic [2:0] pick_w;
  assign pick_w = __out_w_flits_can_send && want_local_w ? 3'd1 : __out_w_flits_can_send && want_n_w ? 3'd2 : __out_w_flits_can_send && want_s_w ? 3'd3 : __out_w_flits_can_send && want_e_w ? 3'd4 : __out_w_flits_can_send && want_w_w ? 3'd5 : 3'd0;
  logic served_local;
  assign served_local = pick_local == 3'd1 || pick_n == 3'd1 || pick_s == 3'd1 || pick_e == 3'd1 || pick_w == 3'd1;
  logic served_n;
  assign served_n = pick_local == 3'd2 || pick_n == 3'd2 || pick_s == 3'd2 || pick_e == 3'd2 || pick_w == 3'd2;
  logic served_s;
  assign served_s = pick_local == 3'd3 || pick_n == 3'd3 || pick_s == 3'd3 || pick_e == 3'd3 || pick_w == 3'd3;
  logic served_e;
  assign served_e = pick_local == 3'd4 || pick_n == 3'd4 || pick_s == 3'd4 || pick_e == 3'd4 || pick_w == 3'd4;
  logic served_w;
  assign served_w = pick_local == 3'd5 || pick_n == 3'd5 || pick_s == 3'd5 || pick_e == 3'd5 || pick_w == 3'd5;
  assign out_local_flits_send_valid = pick_local != 3'd0;
  assign out_local_flits_send_data = pick_local == 3'd1 ? lx_data : pick_local == 3'd2 ? nx_data : pick_local == 3'd3 ? sx_data : pick_local == 3'd4 ? ex_data : wx_data;
  assign out_n_flits_send_valid = pick_n != 3'd0;
  assign out_n_flits_send_data = pick_n == 3'd1 ? lx_data : pick_n == 3'd2 ? nx_data : pick_n == 3'd3 ? sx_data : pick_n == 3'd4 ? ex_data : wx_data;
  assign out_s_flits_send_valid = pick_s != 3'd0;
  assign out_s_flits_send_data = pick_s == 3'd1 ? lx_data : pick_s == 3'd2 ? nx_data : pick_s == 3'd3 ? sx_data : pick_s == 3'd4 ? ex_data : wx_data;
  assign out_e_flits_send_valid = pick_e != 3'd0;
  assign out_e_flits_send_data = pick_e == 3'd1 ? lx_data : pick_e == 3'd2 ? nx_data : pick_e == 3'd3 ? sx_data : pick_e == 3'd4 ? ex_data : wx_data;
  assign out_w_flits_send_valid = pick_w != 3'd0;
  assign out_w_flits_send_data = pick_w == 3'd1 ? lx_data : pick_w == 3'd2 ? nx_data : pick_w == 3'd3 ? sx_data : pick_w == 3'd4 ? ex_data : wx_data;
  assign in_local_flits_credit_return = served_local;
  assign in_n_flits_credit_return = served_n;
  assign in_s_flits_credit_return = served_s;
  assign in_e_flits_credit_return = served_e;
  assign in_w_flits_credit_return = served_w;
  
  // Auto-generated credit_channel state (PR #3b-ii, sender side)
  logic [$clog2((4) + 1) - 1:0] __out_local_flits_credit;
  wire  __out_local_flits_can_send = __out_local_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_local_flits_credit <= 4;
    end else begin
      if (out_local_flits_send_valid && !out_local_flits_credit_return) __out_local_flits_credit <= __out_local_flits_credit - 1;
      else if (out_local_flits_credit_return && !out_local_flits_send_valid) __out_local_flits_credit <= __out_local_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_n_flits_credit;
  wire  __out_n_flits_can_send = __out_n_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_n_flits_credit <= 4;
    end else begin
      if (out_n_flits_send_valid && !out_n_flits_credit_return) __out_n_flits_credit <= __out_n_flits_credit - 1;
      else if (out_n_flits_credit_return && !out_n_flits_send_valid) __out_n_flits_credit <= __out_n_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_s_flits_credit;
  wire  __out_s_flits_can_send = __out_s_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_s_flits_credit <= 4;
    end else begin
      if (out_s_flits_send_valid && !out_s_flits_credit_return) __out_s_flits_credit <= __out_s_flits_credit - 1;
      else if (out_s_flits_credit_return && !out_s_flits_send_valid) __out_s_flits_credit <= __out_s_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_e_flits_credit;
  wire  __out_e_flits_can_send = __out_e_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_e_flits_credit <= 4;
    end else begin
      if (out_e_flits_send_valid && !out_e_flits_credit_return) __out_e_flits_credit <= __out_e_flits_credit - 1;
      else if (out_e_flits_credit_return && !out_e_flits_send_valid) __out_e_flits_credit <= __out_e_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_w_flits_credit;
  wire  __out_w_flits_can_send = __out_w_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_w_flits_credit <= 4;
    end else begin
      if (out_w_flits_send_valid && !out_w_flits_credit_return) __out_w_flits_credit <= __out_w_flits_credit - 1;
      else if (out_w_flits_credit_return && !out_w_flits_send_valid) __out_w_flits_credit <= __out_w_flits_credit + 1;
    end
  end
  
  // Auto-generated credit_channel target-side FIFO (PR #3b-iii)
  logic [(32) - 1:0] __in_local_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_local_flits_occ;
  wire  __in_local_flits_valid = __in_local_flits_occ != 0;
  wire [(32) - 1:0] __in_local_flits_data = __in_local_flits_buf[__in_local_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_local_flits_head <= 0;
      __in_local_flits_tail <= 0;
      __in_local_flits_occ  <= 0;
    end else begin
      if (in_local_flits_send_valid) begin
        __in_local_flits_buf[__in_local_flits_tail] <= in_local_flits_send_data;
        __in_local_flits_tail <= (__in_local_flits_tail + 1) % (4);
      end
      if ((in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_head <= (__in_local_flits_head + 1) % (4);
      if (in_local_flits_send_valid && !(in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ + 1;
      else if (!in_local_flits_send_valid &&  (in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_n_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_n_flits_occ;
  wire  __in_n_flits_valid = __in_n_flits_occ != 0;
  wire [(32) - 1:0] __in_n_flits_data = __in_n_flits_buf[__in_n_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_n_flits_head <= 0;
      __in_n_flits_tail <= 0;
      __in_n_flits_occ  <= 0;
    end else begin
      if (in_n_flits_send_valid) begin
        __in_n_flits_buf[__in_n_flits_tail] <= in_n_flits_send_data;
        __in_n_flits_tail <= (__in_n_flits_tail + 1) % (4);
      end
      if ((in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_head <= (__in_n_flits_head + 1) % (4);
      if (in_n_flits_send_valid && !(in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ + 1;
      else if (!in_n_flits_send_valid &&  (in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_s_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_s_flits_occ;
  wire  __in_s_flits_valid = __in_s_flits_occ != 0;
  wire [(32) - 1:0] __in_s_flits_data = __in_s_flits_buf[__in_s_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_s_flits_head <= 0;
      __in_s_flits_tail <= 0;
      __in_s_flits_occ  <= 0;
    end else begin
      if (in_s_flits_send_valid) begin
        __in_s_flits_buf[__in_s_flits_tail] <= in_s_flits_send_data;
        __in_s_flits_tail <= (__in_s_flits_tail + 1) % (4);
      end
      if ((in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_head <= (__in_s_flits_head + 1) % (4);
      if (in_s_flits_send_valid && !(in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ + 1;
      else if (!in_s_flits_send_valid &&  (in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_e_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_e_flits_occ;
  wire  __in_e_flits_valid = __in_e_flits_occ != 0;
  wire [(32) - 1:0] __in_e_flits_data = __in_e_flits_buf[__in_e_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_e_flits_head <= 0;
      __in_e_flits_tail <= 0;
      __in_e_flits_occ  <= 0;
    end else begin
      if (in_e_flits_send_valid) begin
        __in_e_flits_buf[__in_e_flits_tail] <= in_e_flits_send_data;
        __in_e_flits_tail <= (__in_e_flits_tail + 1) % (4);
      end
      if ((in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_head <= (__in_e_flits_head + 1) % (4);
      if (in_e_flits_send_valid && !(in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ + 1;
      else if (!in_e_flits_send_valid &&  (in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_w_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_w_flits_occ;
  wire  __in_w_flits_valid = __in_w_flits_occ != 0;
  wire [(32) - 1:0] __in_w_flits_data = __in_w_flits_buf[__in_w_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_w_flits_head <= 0;
      __in_w_flits_tail <= 0;
      __in_w_flits_occ  <= 0;
    end else begin
      if (in_w_flits_send_valid) begin
        __in_w_flits_buf[__in_w_flits_tail] <= in_w_flits_send_data;
        __in_w_flits_tail <= (__in_w_flits_tail + 1) % (4);
      end
      if ((in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_head <= (__in_w_flits_head + 1) % (4);
      if (in_w_flits_send_valid && !(in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ + 1;
      else if (!in_w_flits_send_valid &&  (in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ - 1;
    end
  end
  
  // synopsys translate_off
  // Auto-generated credit_channel protocol assertions (Tier 2)
  _auto_cc_out_local_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_local_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_0_Y_1._auto_cc_out_local_flits_credit_bounds");
  _auto_cc_out_local_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_local_flits_send_valid |-> __out_local_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_0_Y_1._auto_cc_out_local_flits_send_requires_credit");
  _auto_cc_out_n_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_n_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_0_Y_1._auto_cc_out_n_flits_credit_bounds");
  _auto_cc_out_n_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_n_flits_send_valid |-> __out_n_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_0_Y_1._auto_cc_out_n_flits_send_requires_credit");
  _auto_cc_out_s_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_s_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_0_Y_1._auto_cc_out_s_flits_credit_bounds");
  _auto_cc_out_s_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_s_flits_send_valid |-> __out_s_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_0_Y_1._auto_cc_out_s_flits_send_requires_credit");
  _auto_cc_out_e_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_e_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_0_Y_1._auto_cc_out_e_flits_credit_bounds");
  _auto_cc_out_e_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_e_flits_send_valid |-> __out_e_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_0_Y_1._auto_cc_out_e_flits_send_requires_credit");
  _auto_cc_out_w_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_w_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_0_Y_1._auto_cc_out_w_flits_credit_bounds");
  _auto_cc_out_w_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_w_flits_send_valid |-> __out_w_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_0_Y_1._auto_cc_out_w_flits_send_requires_credit");
  _auto_cc_in_local_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_local_flits_credit_return |-> __in_local_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_0_Y_1._auto_cc_in_local_flits_credit_return_requires_buffered");
  _auto_cc_in_n_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_n_flits_credit_return |-> __in_n_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_0_Y_1._auto_cc_in_n_flits_credit_return_requires_buffered");
  _auto_cc_in_s_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_s_flits_credit_return |-> __in_s_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_0_Y_1._auto_cc_in_s_flits_credit_return_requires_buffered");
  _auto_cc_in_e_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_e_flits_credit_return |-> __in_e_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_0_Y_1._auto_cc_in_e_flits_credit_return_requires_buffered");
  _auto_cc_in_w_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_w_flits_credit_return |-> __in_w_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_0_Y_1._auto_cc_in_w_flits_credit_return_requires_buffered");
  // synopsys translate_on

endmodule

module Router__X_1_Y_1 #(
  parameter int X = 1,
  parameter int Y = 1
) (
  input logic clk,
  input logic rst,
  input logic in_local_flits_send_valid,
  input logic [31:0] in_local_flits_send_data,
  output logic in_local_flits_credit_return,
  input logic in_n_flits_send_valid,
  input logic [31:0] in_n_flits_send_data,
  output logic in_n_flits_credit_return,
  input logic in_s_flits_send_valid,
  input logic [31:0] in_s_flits_send_data,
  output logic in_s_flits_credit_return,
  input logic in_e_flits_send_valid,
  input logic [31:0] in_e_flits_send_data,
  output logic in_e_flits_credit_return,
  input logic in_w_flits_send_valid,
  input logic [31:0] in_w_flits_send_data,
  output logic in_w_flits_credit_return,
  output logic out_local_flits_send_valid,
  output logic [31:0] out_local_flits_send_data,
  input logic out_local_flits_credit_return,
  output logic out_n_flits_send_valid,
  output logic [31:0] out_n_flits_send_data,
  input logic out_n_flits_credit_return,
  output logic out_s_flits_send_valid,
  output logic [31:0] out_s_flits_send_data,
  input logic out_s_flits_credit_return,
  output logic out_e_flits_send_valid,
  output logic [31:0] out_e_flits_send_data,
  input logic out_e_flits_credit_return,
  output logic out_w_flits_send_valid,
  output logic [31:0] out_w_flits_send_data,
  input logic out_w_flits_credit_return
);

  logic [31:0] lx_data;
  assign lx_data = __in_local_flits_data;
  logic [31:0] nx_data;
  assign nx_data = __in_n_flits_data;
  logic [31:0] sx_data;
  assign sx_data = __in_s_flits_data;
  logic [31:0] ex_data;
  assign ex_data = __in_e_flits_data;
  logic [31:0] wx_data;
  assign wx_data = __in_w_flits_data;
  logic [1:0] lx_dx;
  assign lx_dx = lx_data[1:0];
  logic [1:0] lx_dy;
  assign lx_dy = lx_data[3:2];
  logic [1:0] nx_dx;
  assign nx_dx = nx_data[1:0];
  logic [1:0] nx_dy;
  assign nx_dy = nx_data[3:2];
  logic [1:0] sx_dx;
  assign sx_dx = sx_data[1:0];
  logic [1:0] sx_dy;
  assign sx_dy = sx_data[3:2];
  logic [1:0] ex_dx;
  assign ex_dx = ex_data[1:0];
  logic [1:0] ex_dy;
  assign ex_dy = ex_data[3:2];
  logic [1:0] wx_dx;
  assign wx_dx = wx_data[1:0];
  logic [1:0] wx_dy;
  assign wx_dy = wx_data[3:2];
  logic [1:0] mx;
  assign mx = X;
  logic [1:0] my;
  assign my = Y;
  logic [2:0] route_local;
  assign route_local = lx_dx > mx ? 3'd3 : lx_dx < mx ? 3'd4 : lx_dy > my ? 3'd1 : lx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_n;
  assign route_n = nx_dx > mx ? 3'd3 : nx_dx < mx ? 3'd4 : nx_dy > my ? 3'd1 : nx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_s;
  assign route_s = sx_dx > mx ? 3'd3 : sx_dx < mx ? 3'd4 : sx_dy > my ? 3'd1 : sx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_e;
  assign route_e = ex_dx > mx ? 3'd3 : ex_dx < mx ? 3'd4 : ex_dy > my ? 3'd1 : ex_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_w;
  assign route_w = wx_dx > mx ? 3'd3 : wx_dx < mx ? 3'd4 : wx_dy > my ? 3'd1 : wx_dy < my ? 3'd2 : 3'd0;
  logic want_local_local;
  assign want_local_local = __in_local_flits_valid && route_local == 3'd0;
  logic want_n_local;
  assign want_n_local = __in_n_flits_valid && route_n == 3'd0;
  logic want_s_local;
  assign want_s_local = __in_s_flits_valid && route_s == 3'd0;
  logic want_e_local;
  assign want_e_local = __in_e_flits_valid && route_e == 3'd0;
  logic want_w_local;
  assign want_w_local = __in_w_flits_valid && route_w == 3'd0;
  logic want_local_n;
  assign want_local_n = __in_local_flits_valid && route_local == 3'd1;
  logic want_n_n;
  assign want_n_n = __in_n_flits_valid && route_n == 3'd1;
  logic want_s_n;
  assign want_s_n = __in_s_flits_valid && route_s == 3'd1;
  logic want_e_n;
  assign want_e_n = __in_e_flits_valid && route_e == 3'd1;
  logic want_w_n;
  assign want_w_n = __in_w_flits_valid && route_w == 3'd1;
  logic want_local_s;
  assign want_local_s = __in_local_flits_valid && route_local == 3'd2;
  logic want_n_s;
  assign want_n_s = __in_n_flits_valid && route_n == 3'd2;
  logic want_s_s;
  assign want_s_s = __in_s_flits_valid && route_s == 3'd2;
  logic want_e_s;
  assign want_e_s = __in_e_flits_valid && route_e == 3'd2;
  logic want_w_s;
  assign want_w_s = __in_w_flits_valid && route_w == 3'd2;
  logic want_local_e;
  assign want_local_e = __in_local_flits_valid && route_local == 3'd3;
  logic want_n_e;
  assign want_n_e = __in_n_flits_valid && route_n == 3'd3;
  logic want_s_e;
  assign want_s_e = __in_s_flits_valid && route_s == 3'd3;
  logic want_e_e;
  assign want_e_e = __in_e_flits_valid && route_e == 3'd3;
  logic want_w_e;
  assign want_w_e = __in_w_flits_valid && route_w == 3'd3;
  logic want_local_w;
  assign want_local_w = __in_local_flits_valid && route_local == 3'd4;
  logic want_n_w;
  assign want_n_w = __in_n_flits_valid && route_n == 3'd4;
  logic want_s_w;
  assign want_s_w = __in_s_flits_valid && route_s == 3'd4;
  logic want_e_w;
  assign want_e_w = __in_e_flits_valid && route_e == 3'd4;
  logic want_w_w;
  assign want_w_w = __in_w_flits_valid && route_w == 3'd4;
  logic [2:0] pick_local;
  assign pick_local = __out_local_flits_can_send && want_local_local ? 3'd1 : __out_local_flits_can_send && want_n_local ? 3'd2 : __out_local_flits_can_send && want_s_local ? 3'd3 : __out_local_flits_can_send && want_e_local ? 3'd4 : __out_local_flits_can_send && want_w_local ? 3'd5 : 3'd0;
  logic [2:0] pick_n;
  assign pick_n = __out_n_flits_can_send && want_local_n ? 3'd1 : __out_n_flits_can_send && want_n_n ? 3'd2 : __out_n_flits_can_send && want_s_n ? 3'd3 : __out_n_flits_can_send && want_e_n ? 3'd4 : __out_n_flits_can_send && want_w_n ? 3'd5 : 3'd0;
  logic [2:0] pick_s;
  assign pick_s = __out_s_flits_can_send && want_local_s ? 3'd1 : __out_s_flits_can_send && want_n_s ? 3'd2 : __out_s_flits_can_send && want_s_s ? 3'd3 : __out_s_flits_can_send && want_e_s ? 3'd4 : __out_s_flits_can_send && want_w_s ? 3'd5 : 3'd0;
  logic [2:0] pick_e;
  assign pick_e = __out_e_flits_can_send && want_local_e ? 3'd1 : __out_e_flits_can_send && want_n_e ? 3'd2 : __out_e_flits_can_send && want_s_e ? 3'd3 : __out_e_flits_can_send && want_e_e ? 3'd4 : __out_e_flits_can_send && want_w_e ? 3'd5 : 3'd0;
  logic [2:0] pick_w;
  assign pick_w = __out_w_flits_can_send && want_local_w ? 3'd1 : __out_w_flits_can_send && want_n_w ? 3'd2 : __out_w_flits_can_send && want_s_w ? 3'd3 : __out_w_flits_can_send && want_e_w ? 3'd4 : __out_w_flits_can_send && want_w_w ? 3'd5 : 3'd0;
  logic served_local;
  assign served_local = pick_local == 3'd1 || pick_n == 3'd1 || pick_s == 3'd1 || pick_e == 3'd1 || pick_w == 3'd1;
  logic served_n;
  assign served_n = pick_local == 3'd2 || pick_n == 3'd2 || pick_s == 3'd2 || pick_e == 3'd2 || pick_w == 3'd2;
  logic served_s;
  assign served_s = pick_local == 3'd3 || pick_n == 3'd3 || pick_s == 3'd3 || pick_e == 3'd3 || pick_w == 3'd3;
  logic served_e;
  assign served_e = pick_local == 3'd4 || pick_n == 3'd4 || pick_s == 3'd4 || pick_e == 3'd4 || pick_w == 3'd4;
  logic served_w;
  assign served_w = pick_local == 3'd5 || pick_n == 3'd5 || pick_s == 3'd5 || pick_e == 3'd5 || pick_w == 3'd5;
  assign out_local_flits_send_valid = pick_local != 3'd0;
  assign out_local_flits_send_data = pick_local == 3'd1 ? lx_data : pick_local == 3'd2 ? nx_data : pick_local == 3'd3 ? sx_data : pick_local == 3'd4 ? ex_data : wx_data;
  assign out_n_flits_send_valid = pick_n != 3'd0;
  assign out_n_flits_send_data = pick_n == 3'd1 ? lx_data : pick_n == 3'd2 ? nx_data : pick_n == 3'd3 ? sx_data : pick_n == 3'd4 ? ex_data : wx_data;
  assign out_s_flits_send_valid = pick_s != 3'd0;
  assign out_s_flits_send_data = pick_s == 3'd1 ? lx_data : pick_s == 3'd2 ? nx_data : pick_s == 3'd3 ? sx_data : pick_s == 3'd4 ? ex_data : wx_data;
  assign out_e_flits_send_valid = pick_e != 3'd0;
  assign out_e_flits_send_data = pick_e == 3'd1 ? lx_data : pick_e == 3'd2 ? nx_data : pick_e == 3'd3 ? sx_data : pick_e == 3'd4 ? ex_data : wx_data;
  assign out_w_flits_send_valid = pick_w != 3'd0;
  assign out_w_flits_send_data = pick_w == 3'd1 ? lx_data : pick_w == 3'd2 ? nx_data : pick_w == 3'd3 ? sx_data : pick_w == 3'd4 ? ex_data : wx_data;
  assign in_local_flits_credit_return = served_local;
  assign in_n_flits_credit_return = served_n;
  assign in_s_flits_credit_return = served_s;
  assign in_e_flits_credit_return = served_e;
  assign in_w_flits_credit_return = served_w;
  
  // Auto-generated credit_channel state (PR #3b-ii, sender side)
  logic [$clog2((4) + 1) - 1:0] __out_local_flits_credit;
  wire  __out_local_flits_can_send = __out_local_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_local_flits_credit <= 4;
    end else begin
      if (out_local_flits_send_valid && !out_local_flits_credit_return) __out_local_flits_credit <= __out_local_flits_credit - 1;
      else if (out_local_flits_credit_return && !out_local_flits_send_valid) __out_local_flits_credit <= __out_local_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_n_flits_credit;
  wire  __out_n_flits_can_send = __out_n_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_n_flits_credit <= 4;
    end else begin
      if (out_n_flits_send_valid && !out_n_flits_credit_return) __out_n_flits_credit <= __out_n_flits_credit - 1;
      else if (out_n_flits_credit_return && !out_n_flits_send_valid) __out_n_flits_credit <= __out_n_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_s_flits_credit;
  wire  __out_s_flits_can_send = __out_s_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_s_flits_credit <= 4;
    end else begin
      if (out_s_flits_send_valid && !out_s_flits_credit_return) __out_s_flits_credit <= __out_s_flits_credit - 1;
      else if (out_s_flits_credit_return && !out_s_flits_send_valid) __out_s_flits_credit <= __out_s_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_e_flits_credit;
  wire  __out_e_flits_can_send = __out_e_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_e_flits_credit <= 4;
    end else begin
      if (out_e_flits_send_valid && !out_e_flits_credit_return) __out_e_flits_credit <= __out_e_flits_credit - 1;
      else if (out_e_flits_credit_return && !out_e_flits_send_valid) __out_e_flits_credit <= __out_e_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_w_flits_credit;
  wire  __out_w_flits_can_send = __out_w_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_w_flits_credit <= 4;
    end else begin
      if (out_w_flits_send_valid && !out_w_flits_credit_return) __out_w_flits_credit <= __out_w_flits_credit - 1;
      else if (out_w_flits_credit_return && !out_w_flits_send_valid) __out_w_flits_credit <= __out_w_flits_credit + 1;
    end
  end
  
  // Auto-generated credit_channel target-side FIFO (PR #3b-iii)
  logic [(32) - 1:0] __in_local_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_local_flits_occ;
  wire  __in_local_flits_valid = __in_local_flits_occ != 0;
  wire [(32) - 1:0] __in_local_flits_data = __in_local_flits_buf[__in_local_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_local_flits_head <= 0;
      __in_local_flits_tail <= 0;
      __in_local_flits_occ  <= 0;
    end else begin
      if (in_local_flits_send_valid) begin
        __in_local_flits_buf[__in_local_flits_tail] <= in_local_flits_send_data;
        __in_local_flits_tail <= (__in_local_flits_tail + 1) % (4);
      end
      if ((in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_head <= (__in_local_flits_head + 1) % (4);
      if (in_local_flits_send_valid && !(in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ + 1;
      else if (!in_local_flits_send_valid &&  (in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_n_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_n_flits_occ;
  wire  __in_n_flits_valid = __in_n_flits_occ != 0;
  wire [(32) - 1:0] __in_n_flits_data = __in_n_flits_buf[__in_n_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_n_flits_head <= 0;
      __in_n_flits_tail <= 0;
      __in_n_flits_occ  <= 0;
    end else begin
      if (in_n_flits_send_valid) begin
        __in_n_flits_buf[__in_n_flits_tail] <= in_n_flits_send_data;
        __in_n_flits_tail <= (__in_n_flits_tail + 1) % (4);
      end
      if ((in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_head <= (__in_n_flits_head + 1) % (4);
      if (in_n_flits_send_valid && !(in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ + 1;
      else if (!in_n_flits_send_valid &&  (in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_s_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_s_flits_occ;
  wire  __in_s_flits_valid = __in_s_flits_occ != 0;
  wire [(32) - 1:0] __in_s_flits_data = __in_s_flits_buf[__in_s_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_s_flits_head <= 0;
      __in_s_flits_tail <= 0;
      __in_s_flits_occ  <= 0;
    end else begin
      if (in_s_flits_send_valid) begin
        __in_s_flits_buf[__in_s_flits_tail] <= in_s_flits_send_data;
        __in_s_flits_tail <= (__in_s_flits_tail + 1) % (4);
      end
      if ((in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_head <= (__in_s_flits_head + 1) % (4);
      if (in_s_flits_send_valid && !(in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ + 1;
      else if (!in_s_flits_send_valid &&  (in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_e_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_e_flits_occ;
  wire  __in_e_flits_valid = __in_e_flits_occ != 0;
  wire [(32) - 1:0] __in_e_flits_data = __in_e_flits_buf[__in_e_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_e_flits_head <= 0;
      __in_e_flits_tail <= 0;
      __in_e_flits_occ  <= 0;
    end else begin
      if (in_e_flits_send_valid) begin
        __in_e_flits_buf[__in_e_flits_tail] <= in_e_flits_send_data;
        __in_e_flits_tail <= (__in_e_flits_tail + 1) % (4);
      end
      if ((in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_head <= (__in_e_flits_head + 1) % (4);
      if (in_e_flits_send_valid && !(in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ + 1;
      else if (!in_e_flits_send_valid &&  (in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_w_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_w_flits_occ;
  wire  __in_w_flits_valid = __in_w_flits_occ != 0;
  wire [(32) - 1:0] __in_w_flits_data = __in_w_flits_buf[__in_w_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_w_flits_head <= 0;
      __in_w_flits_tail <= 0;
      __in_w_flits_occ  <= 0;
    end else begin
      if (in_w_flits_send_valid) begin
        __in_w_flits_buf[__in_w_flits_tail] <= in_w_flits_send_data;
        __in_w_flits_tail <= (__in_w_flits_tail + 1) % (4);
      end
      if ((in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_head <= (__in_w_flits_head + 1) % (4);
      if (in_w_flits_send_valid && !(in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ + 1;
      else if (!in_w_flits_send_valid &&  (in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ - 1;
    end
  end
  
  // synopsys translate_off
  // Auto-generated credit_channel protocol assertions (Tier 2)
  _auto_cc_out_local_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_local_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_1_Y_1._auto_cc_out_local_flits_credit_bounds");
  _auto_cc_out_local_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_local_flits_send_valid |-> __out_local_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_1_Y_1._auto_cc_out_local_flits_send_requires_credit");
  _auto_cc_out_n_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_n_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_1_Y_1._auto_cc_out_n_flits_credit_bounds");
  _auto_cc_out_n_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_n_flits_send_valid |-> __out_n_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_1_Y_1._auto_cc_out_n_flits_send_requires_credit");
  _auto_cc_out_s_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_s_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_1_Y_1._auto_cc_out_s_flits_credit_bounds");
  _auto_cc_out_s_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_s_flits_send_valid |-> __out_s_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_1_Y_1._auto_cc_out_s_flits_send_requires_credit");
  _auto_cc_out_e_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_e_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_1_Y_1._auto_cc_out_e_flits_credit_bounds");
  _auto_cc_out_e_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_e_flits_send_valid |-> __out_e_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_1_Y_1._auto_cc_out_e_flits_send_requires_credit");
  _auto_cc_out_w_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_w_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_1_Y_1._auto_cc_out_w_flits_credit_bounds");
  _auto_cc_out_w_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_w_flits_send_valid |-> __out_w_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_1_Y_1._auto_cc_out_w_flits_send_requires_credit");
  _auto_cc_in_local_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_local_flits_credit_return |-> __in_local_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_1_Y_1._auto_cc_in_local_flits_credit_return_requires_buffered");
  _auto_cc_in_n_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_n_flits_credit_return |-> __in_n_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_1_Y_1._auto_cc_in_n_flits_credit_return_requires_buffered");
  _auto_cc_in_s_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_s_flits_credit_return |-> __in_s_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_1_Y_1._auto_cc_in_s_flits_credit_return_requires_buffered");
  _auto_cc_in_e_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_e_flits_credit_return |-> __in_e_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_1_Y_1._auto_cc_in_e_flits_credit_return_requires_buffered");
  _auto_cc_in_w_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_w_flits_credit_return |-> __in_w_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_1_Y_1._auto_cc_in_w_flits_credit_return_requires_buffered");
  // synopsys translate_on

endmodule

module Router__X_2_Y_1 #(
  parameter int X = 2,
  parameter int Y = 1
) (
  input logic clk,
  input logic rst,
  input logic in_local_flits_send_valid,
  input logic [31:0] in_local_flits_send_data,
  output logic in_local_flits_credit_return,
  input logic in_n_flits_send_valid,
  input logic [31:0] in_n_flits_send_data,
  output logic in_n_flits_credit_return,
  input logic in_s_flits_send_valid,
  input logic [31:0] in_s_flits_send_data,
  output logic in_s_flits_credit_return,
  input logic in_e_flits_send_valid,
  input logic [31:0] in_e_flits_send_data,
  output logic in_e_flits_credit_return,
  input logic in_w_flits_send_valid,
  input logic [31:0] in_w_flits_send_data,
  output logic in_w_flits_credit_return,
  output logic out_local_flits_send_valid,
  output logic [31:0] out_local_flits_send_data,
  input logic out_local_flits_credit_return,
  output logic out_n_flits_send_valid,
  output logic [31:0] out_n_flits_send_data,
  input logic out_n_flits_credit_return,
  output logic out_s_flits_send_valid,
  output logic [31:0] out_s_flits_send_data,
  input logic out_s_flits_credit_return,
  output logic out_e_flits_send_valid,
  output logic [31:0] out_e_flits_send_data,
  input logic out_e_flits_credit_return,
  output logic out_w_flits_send_valid,
  output logic [31:0] out_w_flits_send_data,
  input logic out_w_flits_credit_return
);

  logic [31:0] lx_data;
  assign lx_data = __in_local_flits_data;
  logic [31:0] nx_data;
  assign nx_data = __in_n_flits_data;
  logic [31:0] sx_data;
  assign sx_data = __in_s_flits_data;
  logic [31:0] ex_data;
  assign ex_data = __in_e_flits_data;
  logic [31:0] wx_data;
  assign wx_data = __in_w_flits_data;
  logic [1:0] lx_dx;
  assign lx_dx = lx_data[1:0];
  logic [1:0] lx_dy;
  assign lx_dy = lx_data[3:2];
  logic [1:0] nx_dx;
  assign nx_dx = nx_data[1:0];
  logic [1:0] nx_dy;
  assign nx_dy = nx_data[3:2];
  logic [1:0] sx_dx;
  assign sx_dx = sx_data[1:0];
  logic [1:0] sx_dy;
  assign sx_dy = sx_data[3:2];
  logic [1:0] ex_dx;
  assign ex_dx = ex_data[1:0];
  logic [1:0] ex_dy;
  assign ex_dy = ex_data[3:2];
  logic [1:0] wx_dx;
  assign wx_dx = wx_data[1:0];
  logic [1:0] wx_dy;
  assign wx_dy = wx_data[3:2];
  logic [1:0] mx;
  assign mx = X;
  logic [1:0] my;
  assign my = Y;
  logic [2:0] route_local;
  assign route_local = lx_dx > mx ? 3'd3 : lx_dx < mx ? 3'd4 : lx_dy > my ? 3'd1 : lx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_n;
  assign route_n = nx_dx > mx ? 3'd3 : nx_dx < mx ? 3'd4 : nx_dy > my ? 3'd1 : nx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_s;
  assign route_s = sx_dx > mx ? 3'd3 : sx_dx < mx ? 3'd4 : sx_dy > my ? 3'd1 : sx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_e;
  assign route_e = ex_dx > mx ? 3'd3 : ex_dx < mx ? 3'd4 : ex_dy > my ? 3'd1 : ex_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_w;
  assign route_w = wx_dx > mx ? 3'd3 : wx_dx < mx ? 3'd4 : wx_dy > my ? 3'd1 : wx_dy < my ? 3'd2 : 3'd0;
  logic want_local_local;
  assign want_local_local = __in_local_flits_valid && route_local == 3'd0;
  logic want_n_local;
  assign want_n_local = __in_n_flits_valid && route_n == 3'd0;
  logic want_s_local;
  assign want_s_local = __in_s_flits_valid && route_s == 3'd0;
  logic want_e_local;
  assign want_e_local = __in_e_flits_valid && route_e == 3'd0;
  logic want_w_local;
  assign want_w_local = __in_w_flits_valid && route_w == 3'd0;
  logic want_local_n;
  assign want_local_n = __in_local_flits_valid && route_local == 3'd1;
  logic want_n_n;
  assign want_n_n = __in_n_flits_valid && route_n == 3'd1;
  logic want_s_n;
  assign want_s_n = __in_s_flits_valid && route_s == 3'd1;
  logic want_e_n;
  assign want_e_n = __in_e_flits_valid && route_e == 3'd1;
  logic want_w_n;
  assign want_w_n = __in_w_flits_valid && route_w == 3'd1;
  logic want_local_s;
  assign want_local_s = __in_local_flits_valid && route_local == 3'd2;
  logic want_n_s;
  assign want_n_s = __in_n_flits_valid && route_n == 3'd2;
  logic want_s_s;
  assign want_s_s = __in_s_flits_valid && route_s == 3'd2;
  logic want_e_s;
  assign want_e_s = __in_e_flits_valid && route_e == 3'd2;
  logic want_w_s;
  assign want_w_s = __in_w_flits_valid && route_w == 3'd2;
  logic want_local_e;
  assign want_local_e = __in_local_flits_valid && route_local == 3'd3;
  logic want_n_e;
  assign want_n_e = __in_n_flits_valid && route_n == 3'd3;
  logic want_s_e;
  assign want_s_e = __in_s_flits_valid && route_s == 3'd3;
  logic want_e_e;
  assign want_e_e = __in_e_flits_valid && route_e == 3'd3;
  logic want_w_e;
  assign want_w_e = __in_w_flits_valid && route_w == 3'd3;
  logic want_local_w;
  assign want_local_w = __in_local_flits_valid && route_local == 3'd4;
  logic want_n_w;
  assign want_n_w = __in_n_flits_valid && route_n == 3'd4;
  logic want_s_w;
  assign want_s_w = __in_s_flits_valid && route_s == 3'd4;
  logic want_e_w;
  assign want_e_w = __in_e_flits_valid && route_e == 3'd4;
  logic want_w_w;
  assign want_w_w = __in_w_flits_valid && route_w == 3'd4;
  logic [2:0] pick_local;
  assign pick_local = __out_local_flits_can_send && want_local_local ? 3'd1 : __out_local_flits_can_send && want_n_local ? 3'd2 : __out_local_flits_can_send && want_s_local ? 3'd3 : __out_local_flits_can_send && want_e_local ? 3'd4 : __out_local_flits_can_send && want_w_local ? 3'd5 : 3'd0;
  logic [2:0] pick_n;
  assign pick_n = __out_n_flits_can_send && want_local_n ? 3'd1 : __out_n_flits_can_send && want_n_n ? 3'd2 : __out_n_flits_can_send && want_s_n ? 3'd3 : __out_n_flits_can_send && want_e_n ? 3'd4 : __out_n_flits_can_send && want_w_n ? 3'd5 : 3'd0;
  logic [2:0] pick_s;
  assign pick_s = __out_s_flits_can_send && want_local_s ? 3'd1 : __out_s_flits_can_send && want_n_s ? 3'd2 : __out_s_flits_can_send && want_s_s ? 3'd3 : __out_s_flits_can_send && want_e_s ? 3'd4 : __out_s_flits_can_send && want_w_s ? 3'd5 : 3'd0;
  logic [2:0] pick_e;
  assign pick_e = __out_e_flits_can_send && want_local_e ? 3'd1 : __out_e_flits_can_send && want_n_e ? 3'd2 : __out_e_flits_can_send && want_s_e ? 3'd3 : __out_e_flits_can_send && want_e_e ? 3'd4 : __out_e_flits_can_send && want_w_e ? 3'd5 : 3'd0;
  logic [2:0] pick_w;
  assign pick_w = __out_w_flits_can_send && want_local_w ? 3'd1 : __out_w_flits_can_send && want_n_w ? 3'd2 : __out_w_flits_can_send && want_s_w ? 3'd3 : __out_w_flits_can_send && want_e_w ? 3'd4 : __out_w_flits_can_send && want_w_w ? 3'd5 : 3'd0;
  logic served_local;
  assign served_local = pick_local == 3'd1 || pick_n == 3'd1 || pick_s == 3'd1 || pick_e == 3'd1 || pick_w == 3'd1;
  logic served_n;
  assign served_n = pick_local == 3'd2 || pick_n == 3'd2 || pick_s == 3'd2 || pick_e == 3'd2 || pick_w == 3'd2;
  logic served_s;
  assign served_s = pick_local == 3'd3 || pick_n == 3'd3 || pick_s == 3'd3 || pick_e == 3'd3 || pick_w == 3'd3;
  logic served_e;
  assign served_e = pick_local == 3'd4 || pick_n == 3'd4 || pick_s == 3'd4 || pick_e == 3'd4 || pick_w == 3'd4;
  logic served_w;
  assign served_w = pick_local == 3'd5 || pick_n == 3'd5 || pick_s == 3'd5 || pick_e == 3'd5 || pick_w == 3'd5;
  assign out_local_flits_send_valid = pick_local != 3'd0;
  assign out_local_flits_send_data = pick_local == 3'd1 ? lx_data : pick_local == 3'd2 ? nx_data : pick_local == 3'd3 ? sx_data : pick_local == 3'd4 ? ex_data : wx_data;
  assign out_n_flits_send_valid = pick_n != 3'd0;
  assign out_n_flits_send_data = pick_n == 3'd1 ? lx_data : pick_n == 3'd2 ? nx_data : pick_n == 3'd3 ? sx_data : pick_n == 3'd4 ? ex_data : wx_data;
  assign out_s_flits_send_valid = pick_s != 3'd0;
  assign out_s_flits_send_data = pick_s == 3'd1 ? lx_data : pick_s == 3'd2 ? nx_data : pick_s == 3'd3 ? sx_data : pick_s == 3'd4 ? ex_data : wx_data;
  assign out_e_flits_send_valid = pick_e != 3'd0;
  assign out_e_flits_send_data = pick_e == 3'd1 ? lx_data : pick_e == 3'd2 ? nx_data : pick_e == 3'd3 ? sx_data : pick_e == 3'd4 ? ex_data : wx_data;
  assign out_w_flits_send_valid = pick_w != 3'd0;
  assign out_w_flits_send_data = pick_w == 3'd1 ? lx_data : pick_w == 3'd2 ? nx_data : pick_w == 3'd3 ? sx_data : pick_w == 3'd4 ? ex_data : wx_data;
  assign in_local_flits_credit_return = served_local;
  assign in_n_flits_credit_return = served_n;
  assign in_s_flits_credit_return = served_s;
  assign in_e_flits_credit_return = served_e;
  assign in_w_flits_credit_return = served_w;
  
  // Auto-generated credit_channel state (PR #3b-ii, sender side)
  logic [$clog2((4) + 1) - 1:0] __out_local_flits_credit;
  wire  __out_local_flits_can_send = __out_local_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_local_flits_credit <= 4;
    end else begin
      if (out_local_flits_send_valid && !out_local_flits_credit_return) __out_local_flits_credit <= __out_local_flits_credit - 1;
      else if (out_local_flits_credit_return && !out_local_flits_send_valid) __out_local_flits_credit <= __out_local_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_n_flits_credit;
  wire  __out_n_flits_can_send = __out_n_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_n_flits_credit <= 4;
    end else begin
      if (out_n_flits_send_valid && !out_n_flits_credit_return) __out_n_flits_credit <= __out_n_flits_credit - 1;
      else if (out_n_flits_credit_return && !out_n_flits_send_valid) __out_n_flits_credit <= __out_n_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_s_flits_credit;
  wire  __out_s_flits_can_send = __out_s_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_s_flits_credit <= 4;
    end else begin
      if (out_s_flits_send_valid && !out_s_flits_credit_return) __out_s_flits_credit <= __out_s_flits_credit - 1;
      else if (out_s_flits_credit_return && !out_s_flits_send_valid) __out_s_flits_credit <= __out_s_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_e_flits_credit;
  wire  __out_e_flits_can_send = __out_e_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_e_flits_credit <= 4;
    end else begin
      if (out_e_flits_send_valid && !out_e_flits_credit_return) __out_e_flits_credit <= __out_e_flits_credit - 1;
      else if (out_e_flits_credit_return && !out_e_flits_send_valid) __out_e_flits_credit <= __out_e_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_w_flits_credit;
  wire  __out_w_flits_can_send = __out_w_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_w_flits_credit <= 4;
    end else begin
      if (out_w_flits_send_valid && !out_w_flits_credit_return) __out_w_flits_credit <= __out_w_flits_credit - 1;
      else if (out_w_flits_credit_return && !out_w_flits_send_valid) __out_w_flits_credit <= __out_w_flits_credit + 1;
    end
  end
  
  // Auto-generated credit_channel target-side FIFO (PR #3b-iii)
  logic [(32) - 1:0] __in_local_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_local_flits_occ;
  wire  __in_local_flits_valid = __in_local_flits_occ != 0;
  wire [(32) - 1:0] __in_local_flits_data = __in_local_flits_buf[__in_local_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_local_flits_head <= 0;
      __in_local_flits_tail <= 0;
      __in_local_flits_occ  <= 0;
    end else begin
      if (in_local_flits_send_valid) begin
        __in_local_flits_buf[__in_local_flits_tail] <= in_local_flits_send_data;
        __in_local_flits_tail <= (__in_local_flits_tail + 1) % (4);
      end
      if ((in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_head <= (__in_local_flits_head + 1) % (4);
      if (in_local_flits_send_valid && !(in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ + 1;
      else if (!in_local_flits_send_valid &&  (in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_n_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_n_flits_occ;
  wire  __in_n_flits_valid = __in_n_flits_occ != 0;
  wire [(32) - 1:0] __in_n_flits_data = __in_n_flits_buf[__in_n_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_n_flits_head <= 0;
      __in_n_flits_tail <= 0;
      __in_n_flits_occ  <= 0;
    end else begin
      if (in_n_flits_send_valid) begin
        __in_n_flits_buf[__in_n_flits_tail] <= in_n_flits_send_data;
        __in_n_flits_tail <= (__in_n_flits_tail + 1) % (4);
      end
      if ((in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_head <= (__in_n_flits_head + 1) % (4);
      if (in_n_flits_send_valid && !(in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ + 1;
      else if (!in_n_flits_send_valid &&  (in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_s_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_s_flits_occ;
  wire  __in_s_flits_valid = __in_s_flits_occ != 0;
  wire [(32) - 1:0] __in_s_flits_data = __in_s_flits_buf[__in_s_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_s_flits_head <= 0;
      __in_s_flits_tail <= 0;
      __in_s_flits_occ  <= 0;
    end else begin
      if (in_s_flits_send_valid) begin
        __in_s_flits_buf[__in_s_flits_tail] <= in_s_flits_send_data;
        __in_s_flits_tail <= (__in_s_flits_tail + 1) % (4);
      end
      if ((in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_head <= (__in_s_flits_head + 1) % (4);
      if (in_s_flits_send_valid && !(in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ + 1;
      else if (!in_s_flits_send_valid &&  (in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_e_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_e_flits_occ;
  wire  __in_e_flits_valid = __in_e_flits_occ != 0;
  wire [(32) - 1:0] __in_e_flits_data = __in_e_flits_buf[__in_e_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_e_flits_head <= 0;
      __in_e_flits_tail <= 0;
      __in_e_flits_occ  <= 0;
    end else begin
      if (in_e_flits_send_valid) begin
        __in_e_flits_buf[__in_e_flits_tail] <= in_e_flits_send_data;
        __in_e_flits_tail <= (__in_e_flits_tail + 1) % (4);
      end
      if ((in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_head <= (__in_e_flits_head + 1) % (4);
      if (in_e_flits_send_valid && !(in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ + 1;
      else if (!in_e_flits_send_valid &&  (in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_w_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_w_flits_occ;
  wire  __in_w_flits_valid = __in_w_flits_occ != 0;
  wire [(32) - 1:0] __in_w_flits_data = __in_w_flits_buf[__in_w_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_w_flits_head <= 0;
      __in_w_flits_tail <= 0;
      __in_w_flits_occ  <= 0;
    end else begin
      if (in_w_flits_send_valid) begin
        __in_w_flits_buf[__in_w_flits_tail] <= in_w_flits_send_data;
        __in_w_flits_tail <= (__in_w_flits_tail + 1) % (4);
      end
      if ((in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_head <= (__in_w_flits_head + 1) % (4);
      if (in_w_flits_send_valid && !(in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ + 1;
      else if (!in_w_flits_send_valid &&  (in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ - 1;
    end
  end
  
  // synopsys translate_off
  // Auto-generated credit_channel protocol assertions (Tier 2)
  _auto_cc_out_local_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_local_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_2_Y_1._auto_cc_out_local_flits_credit_bounds");
  _auto_cc_out_local_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_local_flits_send_valid |-> __out_local_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_2_Y_1._auto_cc_out_local_flits_send_requires_credit");
  _auto_cc_out_n_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_n_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_2_Y_1._auto_cc_out_n_flits_credit_bounds");
  _auto_cc_out_n_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_n_flits_send_valid |-> __out_n_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_2_Y_1._auto_cc_out_n_flits_send_requires_credit");
  _auto_cc_out_s_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_s_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_2_Y_1._auto_cc_out_s_flits_credit_bounds");
  _auto_cc_out_s_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_s_flits_send_valid |-> __out_s_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_2_Y_1._auto_cc_out_s_flits_send_requires_credit");
  _auto_cc_out_e_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_e_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_2_Y_1._auto_cc_out_e_flits_credit_bounds");
  _auto_cc_out_e_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_e_flits_send_valid |-> __out_e_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_2_Y_1._auto_cc_out_e_flits_send_requires_credit");
  _auto_cc_out_w_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_w_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_2_Y_1._auto_cc_out_w_flits_credit_bounds");
  _auto_cc_out_w_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_w_flits_send_valid |-> __out_w_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_2_Y_1._auto_cc_out_w_flits_send_requires_credit");
  _auto_cc_in_local_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_local_flits_credit_return |-> __in_local_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_2_Y_1._auto_cc_in_local_flits_credit_return_requires_buffered");
  _auto_cc_in_n_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_n_flits_credit_return |-> __in_n_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_2_Y_1._auto_cc_in_n_flits_credit_return_requires_buffered");
  _auto_cc_in_s_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_s_flits_credit_return |-> __in_s_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_2_Y_1._auto_cc_in_s_flits_credit_return_requires_buffered");
  _auto_cc_in_e_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_e_flits_credit_return |-> __in_e_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_2_Y_1._auto_cc_in_e_flits_credit_return_requires_buffered");
  _auto_cc_in_w_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_w_flits_credit_return |-> __in_w_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_2_Y_1._auto_cc_in_w_flits_credit_return_requires_buffered");
  // synopsys translate_on

endmodule

module Router__X_3_Y_1 #(
  parameter int X = 3,
  parameter int Y = 1
) (
  input logic clk,
  input logic rst,
  input logic in_local_flits_send_valid,
  input logic [31:0] in_local_flits_send_data,
  output logic in_local_flits_credit_return,
  input logic in_n_flits_send_valid,
  input logic [31:0] in_n_flits_send_data,
  output logic in_n_flits_credit_return,
  input logic in_s_flits_send_valid,
  input logic [31:0] in_s_flits_send_data,
  output logic in_s_flits_credit_return,
  input logic in_e_flits_send_valid,
  input logic [31:0] in_e_flits_send_data,
  output logic in_e_flits_credit_return,
  input logic in_w_flits_send_valid,
  input logic [31:0] in_w_flits_send_data,
  output logic in_w_flits_credit_return,
  output logic out_local_flits_send_valid,
  output logic [31:0] out_local_flits_send_data,
  input logic out_local_flits_credit_return,
  output logic out_n_flits_send_valid,
  output logic [31:0] out_n_flits_send_data,
  input logic out_n_flits_credit_return,
  output logic out_s_flits_send_valid,
  output logic [31:0] out_s_flits_send_data,
  input logic out_s_flits_credit_return,
  output logic out_e_flits_send_valid,
  output logic [31:0] out_e_flits_send_data,
  input logic out_e_flits_credit_return,
  output logic out_w_flits_send_valid,
  output logic [31:0] out_w_flits_send_data,
  input logic out_w_flits_credit_return
);

  logic [31:0] lx_data;
  assign lx_data = __in_local_flits_data;
  logic [31:0] nx_data;
  assign nx_data = __in_n_flits_data;
  logic [31:0] sx_data;
  assign sx_data = __in_s_flits_data;
  logic [31:0] ex_data;
  assign ex_data = __in_e_flits_data;
  logic [31:0] wx_data;
  assign wx_data = __in_w_flits_data;
  logic [1:0] lx_dx;
  assign lx_dx = lx_data[1:0];
  logic [1:0] lx_dy;
  assign lx_dy = lx_data[3:2];
  logic [1:0] nx_dx;
  assign nx_dx = nx_data[1:0];
  logic [1:0] nx_dy;
  assign nx_dy = nx_data[3:2];
  logic [1:0] sx_dx;
  assign sx_dx = sx_data[1:0];
  logic [1:0] sx_dy;
  assign sx_dy = sx_data[3:2];
  logic [1:0] ex_dx;
  assign ex_dx = ex_data[1:0];
  logic [1:0] ex_dy;
  assign ex_dy = ex_data[3:2];
  logic [1:0] wx_dx;
  assign wx_dx = wx_data[1:0];
  logic [1:0] wx_dy;
  assign wx_dy = wx_data[3:2];
  logic [1:0] mx;
  assign mx = X;
  logic [1:0] my;
  assign my = Y;
  logic [2:0] route_local;
  assign route_local = lx_dx > mx ? 3'd3 : lx_dx < mx ? 3'd4 : lx_dy > my ? 3'd1 : lx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_n;
  assign route_n = nx_dx > mx ? 3'd3 : nx_dx < mx ? 3'd4 : nx_dy > my ? 3'd1 : nx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_s;
  assign route_s = sx_dx > mx ? 3'd3 : sx_dx < mx ? 3'd4 : sx_dy > my ? 3'd1 : sx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_e;
  assign route_e = ex_dx > mx ? 3'd3 : ex_dx < mx ? 3'd4 : ex_dy > my ? 3'd1 : ex_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_w;
  assign route_w = wx_dx > mx ? 3'd3 : wx_dx < mx ? 3'd4 : wx_dy > my ? 3'd1 : wx_dy < my ? 3'd2 : 3'd0;
  logic want_local_local;
  assign want_local_local = __in_local_flits_valid && route_local == 3'd0;
  logic want_n_local;
  assign want_n_local = __in_n_flits_valid && route_n == 3'd0;
  logic want_s_local;
  assign want_s_local = __in_s_flits_valid && route_s == 3'd0;
  logic want_e_local;
  assign want_e_local = __in_e_flits_valid && route_e == 3'd0;
  logic want_w_local;
  assign want_w_local = __in_w_flits_valid && route_w == 3'd0;
  logic want_local_n;
  assign want_local_n = __in_local_flits_valid && route_local == 3'd1;
  logic want_n_n;
  assign want_n_n = __in_n_flits_valid && route_n == 3'd1;
  logic want_s_n;
  assign want_s_n = __in_s_flits_valid && route_s == 3'd1;
  logic want_e_n;
  assign want_e_n = __in_e_flits_valid && route_e == 3'd1;
  logic want_w_n;
  assign want_w_n = __in_w_flits_valid && route_w == 3'd1;
  logic want_local_s;
  assign want_local_s = __in_local_flits_valid && route_local == 3'd2;
  logic want_n_s;
  assign want_n_s = __in_n_flits_valid && route_n == 3'd2;
  logic want_s_s;
  assign want_s_s = __in_s_flits_valid && route_s == 3'd2;
  logic want_e_s;
  assign want_e_s = __in_e_flits_valid && route_e == 3'd2;
  logic want_w_s;
  assign want_w_s = __in_w_flits_valid && route_w == 3'd2;
  logic want_local_e;
  assign want_local_e = __in_local_flits_valid && route_local == 3'd3;
  logic want_n_e;
  assign want_n_e = __in_n_flits_valid && route_n == 3'd3;
  logic want_s_e;
  assign want_s_e = __in_s_flits_valid && route_s == 3'd3;
  logic want_e_e;
  assign want_e_e = __in_e_flits_valid && route_e == 3'd3;
  logic want_w_e;
  assign want_w_e = __in_w_flits_valid && route_w == 3'd3;
  logic want_local_w;
  assign want_local_w = __in_local_flits_valid && route_local == 3'd4;
  logic want_n_w;
  assign want_n_w = __in_n_flits_valid && route_n == 3'd4;
  logic want_s_w;
  assign want_s_w = __in_s_flits_valid && route_s == 3'd4;
  logic want_e_w;
  assign want_e_w = __in_e_flits_valid && route_e == 3'd4;
  logic want_w_w;
  assign want_w_w = __in_w_flits_valid && route_w == 3'd4;
  logic [2:0] pick_local;
  assign pick_local = __out_local_flits_can_send && want_local_local ? 3'd1 : __out_local_flits_can_send && want_n_local ? 3'd2 : __out_local_flits_can_send && want_s_local ? 3'd3 : __out_local_flits_can_send && want_e_local ? 3'd4 : __out_local_flits_can_send && want_w_local ? 3'd5 : 3'd0;
  logic [2:0] pick_n;
  assign pick_n = __out_n_flits_can_send && want_local_n ? 3'd1 : __out_n_flits_can_send && want_n_n ? 3'd2 : __out_n_flits_can_send && want_s_n ? 3'd3 : __out_n_flits_can_send && want_e_n ? 3'd4 : __out_n_flits_can_send && want_w_n ? 3'd5 : 3'd0;
  logic [2:0] pick_s;
  assign pick_s = __out_s_flits_can_send && want_local_s ? 3'd1 : __out_s_flits_can_send && want_n_s ? 3'd2 : __out_s_flits_can_send && want_s_s ? 3'd3 : __out_s_flits_can_send && want_e_s ? 3'd4 : __out_s_flits_can_send && want_w_s ? 3'd5 : 3'd0;
  logic [2:0] pick_e;
  assign pick_e = __out_e_flits_can_send && want_local_e ? 3'd1 : __out_e_flits_can_send && want_n_e ? 3'd2 : __out_e_flits_can_send && want_s_e ? 3'd3 : __out_e_flits_can_send && want_e_e ? 3'd4 : __out_e_flits_can_send && want_w_e ? 3'd5 : 3'd0;
  logic [2:0] pick_w;
  assign pick_w = __out_w_flits_can_send && want_local_w ? 3'd1 : __out_w_flits_can_send && want_n_w ? 3'd2 : __out_w_flits_can_send && want_s_w ? 3'd3 : __out_w_flits_can_send && want_e_w ? 3'd4 : __out_w_flits_can_send && want_w_w ? 3'd5 : 3'd0;
  logic served_local;
  assign served_local = pick_local == 3'd1 || pick_n == 3'd1 || pick_s == 3'd1 || pick_e == 3'd1 || pick_w == 3'd1;
  logic served_n;
  assign served_n = pick_local == 3'd2 || pick_n == 3'd2 || pick_s == 3'd2 || pick_e == 3'd2 || pick_w == 3'd2;
  logic served_s;
  assign served_s = pick_local == 3'd3 || pick_n == 3'd3 || pick_s == 3'd3 || pick_e == 3'd3 || pick_w == 3'd3;
  logic served_e;
  assign served_e = pick_local == 3'd4 || pick_n == 3'd4 || pick_s == 3'd4 || pick_e == 3'd4 || pick_w == 3'd4;
  logic served_w;
  assign served_w = pick_local == 3'd5 || pick_n == 3'd5 || pick_s == 3'd5 || pick_e == 3'd5 || pick_w == 3'd5;
  assign out_local_flits_send_valid = pick_local != 3'd0;
  assign out_local_flits_send_data = pick_local == 3'd1 ? lx_data : pick_local == 3'd2 ? nx_data : pick_local == 3'd3 ? sx_data : pick_local == 3'd4 ? ex_data : wx_data;
  assign out_n_flits_send_valid = pick_n != 3'd0;
  assign out_n_flits_send_data = pick_n == 3'd1 ? lx_data : pick_n == 3'd2 ? nx_data : pick_n == 3'd3 ? sx_data : pick_n == 3'd4 ? ex_data : wx_data;
  assign out_s_flits_send_valid = pick_s != 3'd0;
  assign out_s_flits_send_data = pick_s == 3'd1 ? lx_data : pick_s == 3'd2 ? nx_data : pick_s == 3'd3 ? sx_data : pick_s == 3'd4 ? ex_data : wx_data;
  assign out_e_flits_send_valid = pick_e != 3'd0;
  assign out_e_flits_send_data = pick_e == 3'd1 ? lx_data : pick_e == 3'd2 ? nx_data : pick_e == 3'd3 ? sx_data : pick_e == 3'd4 ? ex_data : wx_data;
  assign out_w_flits_send_valid = pick_w != 3'd0;
  assign out_w_flits_send_data = pick_w == 3'd1 ? lx_data : pick_w == 3'd2 ? nx_data : pick_w == 3'd3 ? sx_data : pick_w == 3'd4 ? ex_data : wx_data;
  assign in_local_flits_credit_return = served_local;
  assign in_n_flits_credit_return = served_n;
  assign in_s_flits_credit_return = served_s;
  assign in_e_flits_credit_return = served_e;
  assign in_w_flits_credit_return = served_w;
  
  // Auto-generated credit_channel state (PR #3b-ii, sender side)
  logic [$clog2((4) + 1) - 1:0] __out_local_flits_credit;
  wire  __out_local_flits_can_send = __out_local_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_local_flits_credit <= 4;
    end else begin
      if (out_local_flits_send_valid && !out_local_flits_credit_return) __out_local_flits_credit <= __out_local_flits_credit - 1;
      else if (out_local_flits_credit_return && !out_local_flits_send_valid) __out_local_flits_credit <= __out_local_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_n_flits_credit;
  wire  __out_n_flits_can_send = __out_n_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_n_flits_credit <= 4;
    end else begin
      if (out_n_flits_send_valid && !out_n_flits_credit_return) __out_n_flits_credit <= __out_n_flits_credit - 1;
      else if (out_n_flits_credit_return && !out_n_flits_send_valid) __out_n_flits_credit <= __out_n_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_s_flits_credit;
  wire  __out_s_flits_can_send = __out_s_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_s_flits_credit <= 4;
    end else begin
      if (out_s_flits_send_valid && !out_s_flits_credit_return) __out_s_flits_credit <= __out_s_flits_credit - 1;
      else if (out_s_flits_credit_return && !out_s_flits_send_valid) __out_s_flits_credit <= __out_s_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_e_flits_credit;
  wire  __out_e_flits_can_send = __out_e_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_e_flits_credit <= 4;
    end else begin
      if (out_e_flits_send_valid && !out_e_flits_credit_return) __out_e_flits_credit <= __out_e_flits_credit - 1;
      else if (out_e_flits_credit_return && !out_e_flits_send_valid) __out_e_flits_credit <= __out_e_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_w_flits_credit;
  wire  __out_w_flits_can_send = __out_w_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_w_flits_credit <= 4;
    end else begin
      if (out_w_flits_send_valid && !out_w_flits_credit_return) __out_w_flits_credit <= __out_w_flits_credit - 1;
      else if (out_w_flits_credit_return && !out_w_flits_send_valid) __out_w_flits_credit <= __out_w_flits_credit + 1;
    end
  end
  
  // Auto-generated credit_channel target-side FIFO (PR #3b-iii)
  logic [(32) - 1:0] __in_local_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_local_flits_occ;
  wire  __in_local_flits_valid = __in_local_flits_occ != 0;
  wire [(32) - 1:0] __in_local_flits_data = __in_local_flits_buf[__in_local_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_local_flits_head <= 0;
      __in_local_flits_tail <= 0;
      __in_local_flits_occ  <= 0;
    end else begin
      if (in_local_flits_send_valid) begin
        __in_local_flits_buf[__in_local_flits_tail] <= in_local_flits_send_data;
        __in_local_flits_tail <= (__in_local_flits_tail + 1) % (4);
      end
      if ((in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_head <= (__in_local_flits_head + 1) % (4);
      if (in_local_flits_send_valid && !(in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ + 1;
      else if (!in_local_flits_send_valid &&  (in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_n_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_n_flits_occ;
  wire  __in_n_flits_valid = __in_n_flits_occ != 0;
  wire [(32) - 1:0] __in_n_flits_data = __in_n_flits_buf[__in_n_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_n_flits_head <= 0;
      __in_n_flits_tail <= 0;
      __in_n_flits_occ  <= 0;
    end else begin
      if (in_n_flits_send_valid) begin
        __in_n_flits_buf[__in_n_flits_tail] <= in_n_flits_send_data;
        __in_n_flits_tail <= (__in_n_flits_tail + 1) % (4);
      end
      if ((in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_head <= (__in_n_flits_head + 1) % (4);
      if (in_n_flits_send_valid && !(in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ + 1;
      else if (!in_n_flits_send_valid &&  (in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_s_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_s_flits_occ;
  wire  __in_s_flits_valid = __in_s_flits_occ != 0;
  wire [(32) - 1:0] __in_s_flits_data = __in_s_flits_buf[__in_s_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_s_flits_head <= 0;
      __in_s_flits_tail <= 0;
      __in_s_flits_occ  <= 0;
    end else begin
      if (in_s_flits_send_valid) begin
        __in_s_flits_buf[__in_s_flits_tail] <= in_s_flits_send_data;
        __in_s_flits_tail <= (__in_s_flits_tail + 1) % (4);
      end
      if ((in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_head <= (__in_s_flits_head + 1) % (4);
      if (in_s_flits_send_valid && !(in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ + 1;
      else if (!in_s_flits_send_valid &&  (in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_e_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_e_flits_occ;
  wire  __in_e_flits_valid = __in_e_flits_occ != 0;
  wire [(32) - 1:0] __in_e_flits_data = __in_e_flits_buf[__in_e_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_e_flits_head <= 0;
      __in_e_flits_tail <= 0;
      __in_e_flits_occ  <= 0;
    end else begin
      if (in_e_flits_send_valid) begin
        __in_e_flits_buf[__in_e_flits_tail] <= in_e_flits_send_data;
        __in_e_flits_tail <= (__in_e_flits_tail + 1) % (4);
      end
      if ((in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_head <= (__in_e_flits_head + 1) % (4);
      if (in_e_flits_send_valid && !(in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ + 1;
      else if (!in_e_flits_send_valid &&  (in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_w_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_w_flits_occ;
  wire  __in_w_flits_valid = __in_w_flits_occ != 0;
  wire [(32) - 1:0] __in_w_flits_data = __in_w_flits_buf[__in_w_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_w_flits_head <= 0;
      __in_w_flits_tail <= 0;
      __in_w_flits_occ  <= 0;
    end else begin
      if (in_w_flits_send_valid) begin
        __in_w_flits_buf[__in_w_flits_tail] <= in_w_flits_send_data;
        __in_w_flits_tail <= (__in_w_flits_tail + 1) % (4);
      end
      if ((in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_head <= (__in_w_flits_head + 1) % (4);
      if (in_w_flits_send_valid && !(in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ + 1;
      else if (!in_w_flits_send_valid &&  (in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ - 1;
    end
  end
  
  // synopsys translate_off
  // Auto-generated credit_channel protocol assertions (Tier 2)
  _auto_cc_out_local_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_local_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_3_Y_1._auto_cc_out_local_flits_credit_bounds");
  _auto_cc_out_local_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_local_flits_send_valid |-> __out_local_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_3_Y_1._auto_cc_out_local_flits_send_requires_credit");
  _auto_cc_out_n_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_n_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_3_Y_1._auto_cc_out_n_flits_credit_bounds");
  _auto_cc_out_n_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_n_flits_send_valid |-> __out_n_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_3_Y_1._auto_cc_out_n_flits_send_requires_credit");
  _auto_cc_out_s_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_s_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_3_Y_1._auto_cc_out_s_flits_credit_bounds");
  _auto_cc_out_s_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_s_flits_send_valid |-> __out_s_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_3_Y_1._auto_cc_out_s_flits_send_requires_credit");
  _auto_cc_out_e_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_e_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_3_Y_1._auto_cc_out_e_flits_credit_bounds");
  _auto_cc_out_e_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_e_flits_send_valid |-> __out_e_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_3_Y_1._auto_cc_out_e_flits_send_requires_credit");
  _auto_cc_out_w_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_w_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_3_Y_1._auto_cc_out_w_flits_credit_bounds");
  _auto_cc_out_w_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_w_flits_send_valid |-> __out_w_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_3_Y_1._auto_cc_out_w_flits_send_requires_credit");
  _auto_cc_in_local_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_local_flits_credit_return |-> __in_local_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_3_Y_1._auto_cc_in_local_flits_credit_return_requires_buffered");
  _auto_cc_in_n_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_n_flits_credit_return |-> __in_n_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_3_Y_1._auto_cc_in_n_flits_credit_return_requires_buffered");
  _auto_cc_in_s_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_s_flits_credit_return |-> __in_s_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_3_Y_1._auto_cc_in_s_flits_credit_return_requires_buffered");
  _auto_cc_in_e_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_e_flits_credit_return |-> __in_e_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_3_Y_1._auto_cc_in_e_flits_credit_return_requires_buffered");
  _auto_cc_in_w_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_w_flits_credit_return |-> __in_w_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_3_Y_1._auto_cc_in_w_flits_credit_return_requires_buffered");
  // synopsys translate_on

endmodule

module Router__X_0_Y_2 #(
  parameter int X = 0,
  parameter int Y = 2
) (
  input logic clk,
  input logic rst,
  input logic in_local_flits_send_valid,
  input logic [31:0] in_local_flits_send_data,
  output logic in_local_flits_credit_return,
  input logic in_n_flits_send_valid,
  input logic [31:0] in_n_flits_send_data,
  output logic in_n_flits_credit_return,
  input logic in_s_flits_send_valid,
  input logic [31:0] in_s_flits_send_data,
  output logic in_s_flits_credit_return,
  input logic in_e_flits_send_valid,
  input logic [31:0] in_e_flits_send_data,
  output logic in_e_flits_credit_return,
  input logic in_w_flits_send_valid,
  input logic [31:0] in_w_flits_send_data,
  output logic in_w_flits_credit_return,
  output logic out_local_flits_send_valid,
  output logic [31:0] out_local_flits_send_data,
  input logic out_local_flits_credit_return,
  output logic out_n_flits_send_valid,
  output logic [31:0] out_n_flits_send_data,
  input logic out_n_flits_credit_return,
  output logic out_s_flits_send_valid,
  output logic [31:0] out_s_flits_send_data,
  input logic out_s_flits_credit_return,
  output logic out_e_flits_send_valid,
  output logic [31:0] out_e_flits_send_data,
  input logic out_e_flits_credit_return,
  output logic out_w_flits_send_valid,
  output logic [31:0] out_w_flits_send_data,
  input logic out_w_flits_credit_return
);

  logic [31:0] lx_data;
  assign lx_data = __in_local_flits_data;
  logic [31:0] nx_data;
  assign nx_data = __in_n_flits_data;
  logic [31:0] sx_data;
  assign sx_data = __in_s_flits_data;
  logic [31:0] ex_data;
  assign ex_data = __in_e_flits_data;
  logic [31:0] wx_data;
  assign wx_data = __in_w_flits_data;
  logic [1:0] lx_dx;
  assign lx_dx = lx_data[1:0];
  logic [1:0] lx_dy;
  assign lx_dy = lx_data[3:2];
  logic [1:0] nx_dx;
  assign nx_dx = nx_data[1:0];
  logic [1:0] nx_dy;
  assign nx_dy = nx_data[3:2];
  logic [1:0] sx_dx;
  assign sx_dx = sx_data[1:0];
  logic [1:0] sx_dy;
  assign sx_dy = sx_data[3:2];
  logic [1:0] ex_dx;
  assign ex_dx = ex_data[1:0];
  logic [1:0] ex_dy;
  assign ex_dy = ex_data[3:2];
  logic [1:0] wx_dx;
  assign wx_dx = wx_data[1:0];
  logic [1:0] wx_dy;
  assign wx_dy = wx_data[3:2];
  logic [1:0] mx;
  assign mx = X;
  logic [1:0] my;
  assign my = Y;
  logic [2:0] route_local;
  assign route_local = lx_dx > mx ? 3'd3 : lx_dx < mx ? 3'd4 : lx_dy > my ? 3'd1 : lx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_n;
  assign route_n = nx_dx > mx ? 3'd3 : nx_dx < mx ? 3'd4 : nx_dy > my ? 3'd1 : nx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_s;
  assign route_s = sx_dx > mx ? 3'd3 : sx_dx < mx ? 3'd4 : sx_dy > my ? 3'd1 : sx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_e;
  assign route_e = ex_dx > mx ? 3'd3 : ex_dx < mx ? 3'd4 : ex_dy > my ? 3'd1 : ex_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_w;
  assign route_w = wx_dx > mx ? 3'd3 : wx_dx < mx ? 3'd4 : wx_dy > my ? 3'd1 : wx_dy < my ? 3'd2 : 3'd0;
  logic want_local_local;
  assign want_local_local = __in_local_flits_valid && route_local == 3'd0;
  logic want_n_local;
  assign want_n_local = __in_n_flits_valid && route_n == 3'd0;
  logic want_s_local;
  assign want_s_local = __in_s_flits_valid && route_s == 3'd0;
  logic want_e_local;
  assign want_e_local = __in_e_flits_valid && route_e == 3'd0;
  logic want_w_local;
  assign want_w_local = __in_w_flits_valid && route_w == 3'd0;
  logic want_local_n;
  assign want_local_n = __in_local_flits_valid && route_local == 3'd1;
  logic want_n_n;
  assign want_n_n = __in_n_flits_valid && route_n == 3'd1;
  logic want_s_n;
  assign want_s_n = __in_s_flits_valid && route_s == 3'd1;
  logic want_e_n;
  assign want_e_n = __in_e_flits_valid && route_e == 3'd1;
  logic want_w_n;
  assign want_w_n = __in_w_flits_valid && route_w == 3'd1;
  logic want_local_s;
  assign want_local_s = __in_local_flits_valid && route_local == 3'd2;
  logic want_n_s;
  assign want_n_s = __in_n_flits_valid && route_n == 3'd2;
  logic want_s_s;
  assign want_s_s = __in_s_flits_valid && route_s == 3'd2;
  logic want_e_s;
  assign want_e_s = __in_e_flits_valid && route_e == 3'd2;
  logic want_w_s;
  assign want_w_s = __in_w_flits_valid && route_w == 3'd2;
  logic want_local_e;
  assign want_local_e = __in_local_flits_valid && route_local == 3'd3;
  logic want_n_e;
  assign want_n_e = __in_n_flits_valid && route_n == 3'd3;
  logic want_s_e;
  assign want_s_e = __in_s_flits_valid && route_s == 3'd3;
  logic want_e_e;
  assign want_e_e = __in_e_flits_valid && route_e == 3'd3;
  logic want_w_e;
  assign want_w_e = __in_w_flits_valid && route_w == 3'd3;
  logic want_local_w;
  assign want_local_w = __in_local_flits_valid && route_local == 3'd4;
  logic want_n_w;
  assign want_n_w = __in_n_flits_valid && route_n == 3'd4;
  logic want_s_w;
  assign want_s_w = __in_s_flits_valid && route_s == 3'd4;
  logic want_e_w;
  assign want_e_w = __in_e_flits_valid && route_e == 3'd4;
  logic want_w_w;
  assign want_w_w = __in_w_flits_valid && route_w == 3'd4;
  logic [2:0] pick_local;
  assign pick_local = __out_local_flits_can_send && want_local_local ? 3'd1 : __out_local_flits_can_send && want_n_local ? 3'd2 : __out_local_flits_can_send && want_s_local ? 3'd3 : __out_local_flits_can_send && want_e_local ? 3'd4 : __out_local_flits_can_send && want_w_local ? 3'd5 : 3'd0;
  logic [2:0] pick_n;
  assign pick_n = __out_n_flits_can_send && want_local_n ? 3'd1 : __out_n_flits_can_send && want_n_n ? 3'd2 : __out_n_flits_can_send && want_s_n ? 3'd3 : __out_n_flits_can_send && want_e_n ? 3'd4 : __out_n_flits_can_send && want_w_n ? 3'd5 : 3'd0;
  logic [2:0] pick_s;
  assign pick_s = __out_s_flits_can_send && want_local_s ? 3'd1 : __out_s_flits_can_send && want_n_s ? 3'd2 : __out_s_flits_can_send && want_s_s ? 3'd3 : __out_s_flits_can_send && want_e_s ? 3'd4 : __out_s_flits_can_send && want_w_s ? 3'd5 : 3'd0;
  logic [2:0] pick_e;
  assign pick_e = __out_e_flits_can_send && want_local_e ? 3'd1 : __out_e_flits_can_send && want_n_e ? 3'd2 : __out_e_flits_can_send && want_s_e ? 3'd3 : __out_e_flits_can_send && want_e_e ? 3'd4 : __out_e_flits_can_send && want_w_e ? 3'd5 : 3'd0;
  logic [2:0] pick_w;
  assign pick_w = __out_w_flits_can_send && want_local_w ? 3'd1 : __out_w_flits_can_send && want_n_w ? 3'd2 : __out_w_flits_can_send && want_s_w ? 3'd3 : __out_w_flits_can_send && want_e_w ? 3'd4 : __out_w_flits_can_send && want_w_w ? 3'd5 : 3'd0;
  logic served_local;
  assign served_local = pick_local == 3'd1 || pick_n == 3'd1 || pick_s == 3'd1 || pick_e == 3'd1 || pick_w == 3'd1;
  logic served_n;
  assign served_n = pick_local == 3'd2 || pick_n == 3'd2 || pick_s == 3'd2 || pick_e == 3'd2 || pick_w == 3'd2;
  logic served_s;
  assign served_s = pick_local == 3'd3 || pick_n == 3'd3 || pick_s == 3'd3 || pick_e == 3'd3 || pick_w == 3'd3;
  logic served_e;
  assign served_e = pick_local == 3'd4 || pick_n == 3'd4 || pick_s == 3'd4 || pick_e == 3'd4 || pick_w == 3'd4;
  logic served_w;
  assign served_w = pick_local == 3'd5 || pick_n == 3'd5 || pick_s == 3'd5 || pick_e == 3'd5 || pick_w == 3'd5;
  assign out_local_flits_send_valid = pick_local != 3'd0;
  assign out_local_flits_send_data = pick_local == 3'd1 ? lx_data : pick_local == 3'd2 ? nx_data : pick_local == 3'd3 ? sx_data : pick_local == 3'd4 ? ex_data : wx_data;
  assign out_n_flits_send_valid = pick_n != 3'd0;
  assign out_n_flits_send_data = pick_n == 3'd1 ? lx_data : pick_n == 3'd2 ? nx_data : pick_n == 3'd3 ? sx_data : pick_n == 3'd4 ? ex_data : wx_data;
  assign out_s_flits_send_valid = pick_s != 3'd0;
  assign out_s_flits_send_data = pick_s == 3'd1 ? lx_data : pick_s == 3'd2 ? nx_data : pick_s == 3'd3 ? sx_data : pick_s == 3'd4 ? ex_data : wx_data;
  assign out_e_flits_send_valid = pick_e != 3'd0;
  assign out_e_flits_send_data = pick_e == 3'd1 ? lx_data : pick_e == 3'd2 ? nx_data : pick_e == 3'd3 ? sx_data : pick_e == 3'd4 ? ex_data : wx_data;
  assign out_w_flits_send_valid = pick_w != 3'd0;
  assign out_w_flits_send_data = pick_w == 3'd1 ? lx_data : pick_w == 3'd2 ? nx_data : pick_w == 3'd3 ? sx_data : pick_w == 3'd4 ? ex_data : wx_data;
  assign in_local_flits_credit_return = served_local;
  assign in_n_flits_credit_return = served_n;
  assign in_s_flits_credit_return = served_s;
  assign in_e_flits_credit_return = served_e;
  assign in_w_flits_credit_return = served_w;
  
  // Auto-generated credit_channel state (PR #3b-ii, sender side)
  logic [$clog2((4) + 1) - 1:0] __out_local_flits_credit;
  wire  __out_local_flits_can_send = __out_local_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_local_flits_credit <= 4;
    end else begin
      if (out_local_flits_send_valid && !out_local_flits_credit_return) __out_local_flits_credit <= __out_local_flits_credit - 1;
      else if (out_local_flits_credit_return && !out_local_flits_send_valid) __out_local_flits_credit <= __out_local_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_n_flits_credit;
  wire  __out_n_flits_can_send = __out_n_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_n_flits_credit <= 4;
    end else begin
      if (out_n_flits_send_valid && !out_n_flits_credit_return) __out_n_flits_credit <= __out_n_flits_credit - 1;
      else if (out_n_flits_credit_return && !out_n_flits_send_valid) __out_n_flits_credit <= __out_n_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_s_flits_credit;
  wire  __out_s_flits_can_send = __out_s_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_s_flits_credit <= 4;
    end else begin
      if (out_s_flits_send_valid && !out_s_flits_credit_return) __out_s_flits_credit <= __out_s_flits_credit - 1;
      else if (out_s_flits_credit_return && !out_s_flits_send_valid) __out_s_flits_credit <= __out_s_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_e_flits_credit;
  wire  __out_e_flits_can_send = __out_e_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_e_flits_credit <= 4;
    end else begin
      if (out_e_flits_send_valid && !out_e_flits_credit_return) __out_e_flits_credit <= __out_e_flits_credit - 1;
      else if (out_e_flits_credit_return && !out_e_flits_send_valid) __out_e_flits_credit <= __out_e_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_w_flits_credit;
  wire  __out_w_flits_can_send = __out_w_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_w_flits_credit <= 4;
    end else begin
      if (out_w_flits_send_valid && !out_w_flits_credit_return) __out_w_flits_credit <= __out_w_flits_credit - 1;
      else if (out_w_flits_credit_return && !out_w_flits_send_valid) __out_w_flits_credit <= __out_w_flits_credit + 1;
    end
  end
  
  // Auto-generated credit_channel target-side FIFO (PR #3b-iii)
  logic [(32) - 1:0] __in_local_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_local_flits_occ;
  wire  __in_local_flits_valid = __in_local_flits_occ != 0;
  wire [(32) - 1:0] __in_local_flits_data = __in_local_flits_buf[__in_local_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_local_flits_head <= 0;
      __in_local_flits_tail <= 0;
      __in_local_flits_occ  <= 0;
    end else begin
      if (in_local_flits_send_valid) begin
        __in_local_flits_buf[__in_local_flits_tail] <= in_local_flits_send_data;
        __in_local_flits_tail <= (__in_local_flits_tail + 1) % (4);
      end
      if ((in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_head <= (__in_local_flits_head + 1) % (4);
      if (in_local_flits_send_valid && !(in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ + 1;
      else if (!in_local_flits_send_valid &&  (in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_n_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_n_flits_occ;
  wire  __in_n_flits_valid = __in_n_flits_occ != 0;
  wire [(32) - 1:0] __in_n_flits_data = __in_n_flits_buf[__in_n_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_n_flits_head <= 0;
      __in_n_flits_tail <= 0;
      __in_n_flits_occ  <= 0;
    end else begin
      if (in_n_flits_send_valid) begin
        __in_n_flits_buf[__in_n_flits_tail] <= in_n_flits_send_data;
        __in_n_flits_tail <= (__in_n_flits_tail + 1) % (4);
      end
      if ((in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_head <= (__in_n_flits_head + 1) % (4);
      if (in_n_flits_send_valid && !(in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ + 1;
      else if (!in_n_flits_send_valid &&  (in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_s_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_s_flits_occ;
  wire  __in_s_flits_valid = __in_s_flits_occ != 0;
  wire [(32) - 1:0] __in_s_flits_data = __in_s_flits_buf[__in_s_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_s_flits_head <= 0;
      __in_s_flits_tail <= 0;
      __in_s_flits_occ  <= 0;
    end else begin
      if (in_s_flits_send_valid) begin
        __in_s_flits_buf[__in_s_flits_tail] <= in_s_flits_send_data;
        __in_s_flits_tail <= (__in_s_flits_tail + 1) % (4);
      end
      if ((in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_head <= (__in_s_flits_head + 1) % (4);
      if (in_s_flits_send_valid && !(in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ + 1;
      else if (!in_s_flits_send_valid &&  (in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_e_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_e_flits_occ;
  wire  __in_e_flits_valid = __in_e_flits_occ != 0;
  wire [(32) - 1:0] __in_e_flits_data = __in_e_flits_buf[__in_e_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_e_flits_head <= 0;
      __in_e_flits_tail <= 0;
      __in_e_flits_occ  <= 0;
    end else begin
      if (in_e_flits_send_valid) begin
        __in_e_flits_buf[__in_e_flits_tail] <= in_e_flits_send_data;
        __in_e_flits_tail <= (__in_e_flits_tail + 1) % (4);
      end
      if ((in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_head <= (__in_e_flits_head + 1) % (4);
      if (in_e_flits_send_valid && !(in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ + 1;
      else if (!in_e_flits_send_valid &&  (in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_w_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_w_flits_occ;
  wire  __in_w_flits_valid = __in_w_flits_occ != 0;
  wire [(32) - 1:0] __in_w_flits_data = __in_w_flits_buf[__in_w_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_w_flits_head <= 0;
      __in_w_flits_tail <= 0;
      __in_w_flits_occ  <= 0;
    end else begin
      if (in_w_flits_send_valid) begin
        __in_w_flits_buf[__in_w_flits_tail] <= in_w_flits_send_data;
        __in_w_flits_tail <= (__in_w_flits_tail + 1) % (4);
      end
      if ((in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_head <= (__in_w_flits_head + 1) % (4);
      if (in_w_flits_send_valid && !(in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ + 1;
      else if (!in_w_flits_send_valid &&  (in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ - 1;
    end
  end
  
  // synopsys translate_off
  // Auto-generated credit_channel protocol assertions (Tier 2)
  _auto_cc_out_local_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_local_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_0_Y_2._auto_cc_out_local_flits_credit_bounds");
  _auto_cc_out_local_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_local_flits_send_valid |-> __out_local_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_0_Y_2._auto_cc_out_local_flits_send_requires_credit");
  _auto_cc_out_n_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_n_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_0_Y_2._auto_cc_out_n_flits_credit_bounds");
  _auto_cc_out_n_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_n_flits_send_valid |-> __out_n_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_0_Y_2._auto_cc_out_n_flits_send_requires_credit");
  _auto_cc_out_s_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_s_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_0_Y_2._auto_cc_out_s_flits_credit_bounds");
  _auto_cc_out_s_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_s_flits_send_valid |-> __out_s_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_0_Y_2._auto_cc_out_s_flits_send_requires_credit");
  _auto_cc_out_e_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_e_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_0_Y_2._auto_cc_out_e_flits_credit_bounds");
  _auto_cc_out_e_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_e_flits_send_valid |-> __out_e_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_0_Y_2._auto_cc_out_e_flits_send_requires_credit");
  _auto_cc_out_w_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_w_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_0_Y_2._auto_cc_out_w_flits_credit_bounds");
  _auto_cc_out_w_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_w_flits_send_valid |-> __out_w_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_0_Y_2._auto_cc_out_w_flits_send_requires_credit");
  _auto_cc_in_local_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_local_flits_credit_return |-> __in_local_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_0_Y_2._auto_cc_in_local_flits_credit_return_requires_buffered");
  _auto_cc_in_n_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_n_flits_credit_return |-> __in_n_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_0_Y_2._auto_cc_in_n_flits_credit_return_requires_buffered");
  _auto_cc_in_s_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_s_flits_credit_return |-> __in_s_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_0_Y_2._auto_cc_in_s_flits_credit_return_requires_buffered");
  _auto_cc_in_e_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_e_flits_credit_return |-> __in_e_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_0_Y_2._auto_cc_in_e_flits_credit_return_requires_buffered");
  _auto_cc_in_w_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_w_flits_credit_return |-> __in_w_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_0_Y_2._auto_cc_in_w_flits_credit_return_requires_buffered");
  // synopsys translate_on

endmodule

module Router__X_1_Y_2 #(
  parameter int X = 1,
  parameter int Y = 2
) (
  input logic clk,
  input logic rst,
  input logic in_local_flits_send_valid,
  input logic [31:0] in_local_flits_send_data,
  output logic in_local_flits_credit_return,
  input logic in_n_flits_send_valid,
  input logic [31:0] in_n_flits_send_data,
  output logic in_n_flits_credit_return,
  input logic in_s_flits_send_valid,
  input logic [31:0] in_s_flits_send_data,
  output logic in_s_flits_credit_return,
  input logic in_e_flits_send_valid,
  input logic [31:0] in_e_flits_send_data,
  output logic in_e_flits_credit_return,
  input logic in_w_flits_send_valid,
  input logic [31:0] in_w_flits_send_data,
  output logic in_w_flits_credit_return,
  output logic out_local_flits_send_valid,
  output logic [31:0] out_local_flits_send_data,
  input logic out_local_flits_credit_return,
  output logic out_n_flits_send_valid,
  output logic [31:0] out_n_flits_send_data,
  input logic out_n_flits_credit_return,
  output logic out_s_flits_send_valid,
  output logic [31:0] out_s_flits_send_data,
  input logic out_s_flits_credit_return,
  output logic out_e_flits_send_valid,
  output logic [31:0] out_e_flits_send_data,
  input logic out_e_flits_credit_return,
  output logic out_w_flits_send_valid,
  output logic [31:0] out_w_flits_send_data,
  input logic out_w_flits_credit_return
);

  logic [31:0] lx_data;
  assign lx_data = __in_local_flits_data;
  logic [31:0] nx_data;
  assign nx_data = __in_n_flits_data;
  logic [31:0] sx_data;
  assign sx_data = __in_s_flits_data;
  logic [31:0] ex_data;
  assign ex_data = __in_e_flits_data;
  logic [31:0] wx_data;
  assign wx_data = __in_w_flits_data;
  logic [1:0] lx_dx;
  assign lx_dx = lx_data[1:0];
  logic [1:0] lx_dy;
  assign lx_dy = lx_data[3:2];
  logic [1:0] nx_dx;
  assign nx_dx = nx_data[1:0];
  logic [1:0] nx_dy;
  assign nx_dy = nx_data[3:2];
  logic [1:0] sx_dx;
  assign sx_dx = sx_data[1:0];
  logic [1:0] sx_dy;
  assign sx_dy = sx_data[3:2];
  logic [1:0] ex_dx;
  assign ex_dx = ex_data[1:0];
  logic [1:0] ex_dy;
  assign ex_dy = ex_data[3:2];
  logic [1:0] wx_dx;
  assign wx_dx = wx_data[1:0];
  logic [1:0] wx_dy;
  assign wx_dy = wx_data[3:2];
  logic [1:0] mx;
  assign mx = X;
  logic [1:0] my;
  assign my = Y;
  logic [2:0] route_local;
  assign route_local = lx_dx > mx ? 3'd3 : lx_dx < mx ? 3'd4 : lx_dy > my ? 3'd1 : lx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_n;
  assign route_n = nx_dx > mx ? 3'd3 : nx_dx < mx ? 3'd4 : nx_dy > my ? 3'd1 : nx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_s;
  assign route_s = sx_dx > mx ? 3'd3 : sx_dx < mx ? 3'd4 : sx_dy > my ? 3'd1 : sx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_e;
  assign route_e = ex_dx > mx ? 3'd3 : ex_dx < mx ? 3'd4 : ex_dy > my ? 3'd1 : ex_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_w;
  assign route_w = wx_dx > mx ? 3'd3 : wx_dx < mx ? 3'd4 : wx_dy > my ? 3'd1 : wx_dy < my ? 3'd2 : 3'd0;
  logic want_local_local;
  assign want_local_local = __in_local_flits_valid && route_local == 3'd0;
  logic want_n_local;
  assign want_n_local = __in_n_flits_valid && route_n == 3'd0;
  logic want_s_local;
  assign want_s_local = __in_s_flits_valid && route_s == 3'd0;
  logic want_e_local;
  assign want_e_local = __in_e_flits_valid && route_e == 3'd0;
  logic want_w_local;
  assign want_w_local = __in_w_flits_valid && route_w == 3'd0;
  logic want_local_n;
  assign want_local_n = __in_local_flits_valid && route_local == 3'd1;
  logic want_n_n;
  assign want_n_n = __in_n_flits_valid && route_n == 3'd1;
  logic want_s_n;
  assign want_s_n = __in_s_flits_valid && route_s == 3'd1;
  logic want_e_n;
  assign want_e_n = __in_e_flits_valid && route_e == 3'd1;
  logic want_w_n;
  assign want_w_n = __in_w_flits_valid && route_w == 3'd1;
  logic want_local_s;
  assign want_local_s = __in_local_flits_valid && route_local == 3'd2;
  logic want_n_s;
  assign want_n_s = __in_n_flits_valid && route_n == 3'd2;
  logic want_s_s;
  assign want_s_s = __in_s_flits_valid && route_s == 3'd2;
  logic want_e_s;
  assign want_e_s = __in_e_flits_valid && route_e == 3'd2;
  logic want_w_s;
  assign want_w_s = __in_w_flits_valid && route_w == 3'd2;
  logic want_local_e;
  assign want_local_e = __in_local_flits_valid && route_local == 3'd3;
  logic want_n_e;
  assign want_n_e = __in_n_flits_valid && route_n == 3'd3;
  logic want_s_e;
  assign want_s_e = __in_s_flits_valid && route_s == 3'd3;
  logic want_e_e;
  assign want_e_e = __in_e_flits_valid && route_e == 3'd3;
  logic want_w_e;
  assign want_w_e = __in_w_flits_valid && route_w == 3'd3;
  logic want_local_w;
  assign want_local_w = __in_local_flits_valid && route_local == 3'd4;
  logic want_n_w;
  assign want_n_w = __in_n_flits_valid && route_n == 3'd4;
  logic want_s_w;
  assign want_s_w = __in_s_flits_valid && route_s == 3'd4;
  logic want_e_w;
  assign want_e_w = __in_e_flits_valid && route_e == 3'd4;
  logic want_w_w;
  assign want_w_w = __in_w_flits_valid && route_w == 3'd4;
  logic [2:0] pick_local;
  assign pick_local = __out_local_flits_can_send && want_local_local ? 3'd1 : __out_local_flits_can_send && want_n_local ? 3'd2 : __out_local_flits_can_send && want_s_local ? 3'd3 : __out_local_flits_can_send && want_e_local ? 3'd4 : __out_local_flits_can_send && want_w_local ? 3'd5 : 3'd0;
  logic [2:0] pick_n;
  assign pick_n = __out_n_flits_can_send && want_local_n ? 3'd1 : __out_n_flits_can_send && want_n_n ? 3'd2 : __out_n_flits_can_send && want_s_n ? 3'd3 : __out_n_flits_can_send && want_e_n ? 3'd4 : __out_n_flits_can_send && want_w_n ? 3'd5 : 3'd0;
  logic [2:0] pick_s;
  assign pick_s = __out_s_flits_can_send && want_local_s ? 3'd1 : __out_s_flits_can_send && want_n_s ? 3'd2 : __out_s_flits_can_send && want_s_s ? 3'd3 : __out_s_flits_can_send && want_e_s ? 3'd4 : __out_s_flits_can_send && want_w_s ? 3'd5 : 3'd0;
  logic [2:0] pick_e;
  assign pick_e = __out_e_flits_can_send && want_local_e ? 3'd1 : __out_e_flits_can_send && want_n_e ? 3'd2 : __out_e_flits_can_send && want_s_e ? 3'd3 : __out_e_flits_can_send && want_e_e ? 3'd4 : __out_e_flits_can_send && want_w_e ? 3'd5 : 3'd0;
  logic [2:0] pick_w;
  assign pick_w = __out_w_flits_can_send && want_local_w ? 3'd1 : __out_w_flits_can_send && want_n_w ? 3'd2 : __out_w_flits_can_send && want_s_w ? 3'd3 : __out_w_flits_can_send && want_e_w ? 3'd4 : __out_w_flits_can_send && want_w_w ? 3'd5 : 3'd0;
  logic served_local;
  assign served_local = pick_local == 3'd1 || pick_n == 3'd1 || pick_s == 3'd1 || pick_e == 3'd1 || pick_w == 3'd1;
  logic served_n;
  assign served_n = pick_local == 3'd2 || pick_n == 3'd2 || pick_s == 3'd2 || pick_e == 3'd2 || pick_w == 3'd2;
  logic served_s;
  assign served_s = pick_local == 3'd3 || pick_n == 3'd3 || pick_s == 3'd3 || pick_e == 3'd3 || pick_w == 3'd3;
  logic served_e;
  assign served_e = pick_local == 3'd4 || pick_n == 3'd4 || pick_s == 3'd4 || pick_e == 3'd4 || pick_w == 3'd4;
  logic served_w;
  assign served_w = pick_local == 3'd5 || pick_n == 3'd5 || pick_s == 3'd5 || pick_e == 3'd5 || pick_w == 3'd5;
  assign out_local_flits_send_valid = pick_local != 3'd0;
  assign out_local_flits_send_data = pick_local == 3'd1 ? lx_data : pick_local == 3'd2 ? nx_data : pick_local == 3'd3 ? sx_data : pick_local == 3'd4 ? ex_data : wx_data;
  assign out_n_flits_send_valid = pick_n != 3'd0;
  assign out_n_flits_send_data = pick_n == 3'd1 ? lx_data : pick_n == 3'd2 ? nx_data : pick_n == 3'd3 ? sx_data : pick_n == 3'd4 ? ex_data : wx_data;
  assign out_s_flits_send_valid = pick_s != 3'd0;
  assign out_s_flits_send_data = pick_s == 3'd1 ? lx_data : pick_s == 3'd2 ? nx_data : pick_s == 3'd3 ? sx_data : pick_s == 3'd4 ? ex_data : wx_data;
  assign out_e_flits_send_valid = pick_e != 3'd0;
  assign out_e_flits_send_data = pick_e == 3'd1 ? lx_data : pick_e == 3'd2 ? nx_data : pick_e == 3'd3 ? sx_data : pick_e == 3'd4 ? ex_data : wx_data;
  assign out_w_flits_send_valid = pick_w != 3'd0;
  assign out_w_flits_send_data = pick_w == 3'd1 ? lx_data : pick_w == 3'd2 ? nx_data : pick_w == 3'd3 ? sx_data : pick_w == 3'd4 ? ex_data : wx_data;
  assign in_local_flits_credit_return = served_local;
  assign in_n_flits_credit_return = served_n;
  assign in_s_flits_credit_return = served_s;
  assign in_e_flits_credit_return = served_e;
  assign in_w_flits_credit_return = served_w;
  
  // Auto-generated credit_channel state (PR #3b-ii, sender side)
  logic [$clog2((4) + 1) - 1:0] __out_local_flits_credit;
  wire  __out_local_flits_can_send = __out_local_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_local_flits_credit <= 4;
    end else begin
      if (out_local_flits_send_valid && !out_local_flits_credit_return) __out_local_flits_credit <= __out_local_flits_credit - 1;
      else if (out_local_flits_credit_return && !out_local_flits_send_valid) __out_local_flits_credit <= __out_local_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_n_flits_credit;
  wire  __out_n_flits_can_send = __out_n_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_n_flits_credit <= 4;
    end else begin
      if (out_n_flits_send_valid && !out_n_flits_credit_return) __out_n_flits_credit <= __out_n_flits_credit - 1;
      else if (out_n_flits_credit_return && !out_n_flits_send_valid) __out_n_flits_credit <= __out_n_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_s_flits_credit;
  wire  __out_s_flits_can_send = __out_s_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_s_flits_credit <= 4;
    end else begin
      if (out_s_flits_send_valid && !out_s_flits_credit_return) __out_s_flits_credit <= __out_s_flits_credit - 1;
      else if (out_s_flits_credit_return && !out_s_flits_send_valid) __out_s_flits_credit <= __out_s_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_e_flits_credit;
  wire  __out_e_flits_can_send = __out_e_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_e_flits_credit <= 4;
    end else begin
      if (out_e_flits_send_valid && !out_e_flits_credit_return) __out_e_flits_credit <= __out_e_flits_credit - 1;
      else if (out_e_flits_credit_return && !out_e_flits_send_valid) __out_e_flits_credit <= __out_e_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_w_flits_credit;
  wire  __out_w_flits_can_send = __out_w_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_w_flits_credit <= 4;
    end else begin
      if (out_w_flits_send_valid && !out_w_flits_credit_return) __out_w_flits_credit <= __out_w_flits_credit - 1;
      else if (out_w_flits_credit_return && !out_w_flits_send_valid) __out_w_flits_credit <= __out_w_flits_credit + 1;
    end
  end
  
  // Auto-generated credit_channel target-side FIFO (PR #3b-iii)
  logic [(32) - 1:0] __in_local_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_local_flits_occ;
  wire  __in_local_flits_valid = __in_local_flits_occ != 0;
  wire [(32) - 1:0] __in_local_flits_data = __in_local_flits_buf[__in_local_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_local_flits_head <= 0;
      __in_local_flits_tail <= 0;
      __in_local_flits_occ  <= 0;
    end else begin
      if (in_local_flits_send_valid) begin
        __in_local_flits_buf[__in_local_flits_tail] <= in_local_flits_send_data;
        __in_local_flits_tail <= (__in_local_flits_tail + 1) % (4);
      end
      if ((in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_head <= (__in_local_flits_head + 1) % (4);
      if (in_local_flits_send_valid && !(in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ + 1;
      else if (!in_local_flits_send_valid &&  (in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_n_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_n_flits_occ;
  wire  __in_n_flits_valid = __in_n_flits_occ != 0;
  wire [(32) - 1:0] __in_n_flits_data = __in_n_flits_buf[__in_n_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_n_flits_head <= 0;
      __in_n_flits_tail <= 0;
      __in_n_flits_occ  <= 0;
    end else begin
      if (in_n_flits_send_valid) begin
        __in_n_flits_buf[__in_n_flits_tail] <= in_n_flits_send_data;
        __in_n_flits_tail <= (__in_n_flits_tail + 1) % (4);
      end
      if ((in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_head <= (__in_n_flits_head + 1) % (4);
      if (in_n_flits_send_valid && !(in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ + 1;
      else if (!in_n_flits_send_valid &&  (in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_s_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_s_flits_occ;
  wire  __in_s_flits_valid = __in_s_flits_occ != 0;
  wire [(32) - 1:0] __in_s_flits_data = __in_s_flits_buf[__in_s_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_s_flits_head <= 0;
      __in_s_flits_tail <= 0;
      __in_s_flits_occ  <= 0;
    end else begin
      if (in_s_flits_send_valid) begin
        __in_s_flits_buf[__in_s_flits_tail] <= in_s_flits_send_data;
        __in_s_flits_tail <= (__in_s_flits_tail + 1) % (4);
      end
      if ((in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_head <= (__in_s_flits_head + 1) % (4);
      if (in_s_flits_send_valid && !(in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ + 1;
      else if (!in_s_flits_send_valid &&  (in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_e_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_e_flits_occ;
  wire  __in_e_flits_valid = __in_e_flits_occ != 0;
  wire [(32) - 1:0] __in_e_flits_data = __in_e_flits_buf[__in_e_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_e_flits_head <= 0;
      __in_e_flits_tail <= 0;
      __in_e_flits_occ  <= 0;
    end else begin
      if (in_e_flits_send_valid) begin
        __in_e_flits_buf[__in_e_flits_tail] <= in_e_flits_send_data;
        __in_e_flits_tail <= (__in_e_flits_tail + 1) % (4);
      end
      if ((in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_head <= (__in_e_flits_head + 1) % (4);
      if (in_e_flits_send_valid && !(in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ + 1;
      else if (!in_e_flits_send_valid &&  (in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_w_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_w_flits_occ;
  wire  __in_w_flits_valid = __in_w_flits_occ != 0;
  wire [(32) - 1:0] __in_w_flits_data = __in_w_flits_buf[__in_w_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_w_flits_head <= 0;
      __in_w_flits_tail <= 0;
      __in_w_flits_occ  <= 0;
    end else begin
      if (in_w_flits_send_valid) begin
        __in_w_flits_buf[__in_w_flits_tail] <= in_w_flits_send_data;
        __in_w_flits_tail <= (__in_w_flits_tail + 1) % (4);
      end
      if ((in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_head <= (__in_w_flits_head + 1) % (4);
      if (in_w_flits_send_valid && !(in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ + 1;
      else if (!in_w_flits_send_valid &&  (in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ - 1;
    end
  end
  
  // synopsys translate_off
  // Auto-generated credit_channel protocol assertions (Tier 2)
  _auto_cc_out_local_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_local_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_1_Y_2._auto_cc_out_local_flits_credit_bounds");
  _auto_cc_out_local_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_local_flits_send_valid |-> __out_local_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_1_Y_2._auto_cc_out_local_flits_send_requires_credit");
  _auto_cc_out_n_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_n_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_1_Y_2._auto_cc_out_n_flits_credit_bounds");
  _auto_cc_out_n_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_n_flits_send_valid |-> __out_n_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_1_Y_2._auto_cc_out_n_flits_send_requires_credit");
  _auto_cc_out_s_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_s_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_1_Y_2._auto_cc_out_s_flits_credit_bounds");
  _auto_cc_out_s_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_s_flits_send_valid |-> __out_s_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_1_Y_2._auto_cc_out_s_flits_send_requires_credit");
  _auto_cc_out_e_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_e_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_1_Y_2._auto_cc_out_e_flits_credit_bounds");
  _auto_cc_out_e_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_e_flits_send_valid |-> __out_e_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_1_Y_2._auto_cc_out_e_flits_send_requires_credit");
  _auto_cc_out_w_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_w_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_1_Y_2._auto_cc_out_w_flits_credit_bounds");
  _auto_cc_out_w_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_w_flits_send_valid |-> __out_w_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_1_Y_2._auto_cc_out_w_flits_send_requires_credit");
  _auto_cc_in_local_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_local_flits_credit_return |-> __in_local_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_1_Y_2._auto_cc_in_local_flits_credit_return_requires_buffered");
  _auto_cc_in_n_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_n_flits_credit_return |-> __in_n_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_1_Y_2._auto_cc_in_n_flits_credit_return_requires_buffered");
  _auto_cc_in_s_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_s_flits_credit_return |-> __in_s_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_1_Y_2._auto_cc_in_s_flits_credit_return_requires_buffered");
  _auto_cc_in_e_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_e_flits_credit_return |-> __in_e_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_1_Y_2._auto_cc_in_e_flits_credit_return_requires_buffered");
  _auto_cc_in_w_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_w_flits_credit_return |-> __in_w_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_1_Y_2._auto_cc_in_w_flits_credit_return_requires_buffered");
  // synopsys translate_on

endmodule

module Router__X_2_Y_2 #(
  parameter int X = 2,
  parameter int Y = 2
) (
  input logic clk,
  input logic rst,
  input logic in_local_flits_send_valid,
  input logic [31:0] in_local_flits_send_data,
  output logic in_local_flits_credit_return,
  input logic in_n_flits_send_valid,
  input logic [31:0] in_n_flits_send_data,
  output logic in_n_flits_credit_return,
  input logic in_s_flits_send_valid,
  input logic [31:0] in_s_flits_send_data,
  output logic in_s_flits_credit_return,
  input logic in_e_flits_send_valid,
  input logic [31:0] in_e_flits_send_data,
  output logic in_e_flits_credit_return,
  input logic in_w_flits_send_valid,
  input logic [31:0] in_w_flits_send_data,
  output logic in_w_flits_credit_return,
  output logic out_local_flits_send_valid,
  output logic [31:0] out_local_flits_send_data,
  input logic out_local_flits_credit_return,
  output logic out_n_flits_send_valid,
  output logic [31:0] out_n_flits_send_data,
  input logic out_n_flits_credit_return,
  output logic out_s_flits_send_valid,
  output logic [31:0] out_s_flits_send_data,
  input logic out_s_flits_credit_return,
  output logic out_e_flits_send_valid,
  output logic [31:0] out_e_flits_send_data,
  input logic out_e_flits_credit_return,
  output logic out_w_flits_send_valid,
  output logic [31:0] out_w_flits_send_data,
  input logic out_w_flits_credit_return
);

  logic [31:0] lx_data;
  assign lx_data = __in_local_flits_data;
  logic [31:0] nx_data;
  assign nx_data = __in_n_flits_data;
  logic [31:0] sx_data;
  assign sx_data = __in_s_flits_data;
  logic [31:0] ex_data;
  assign ex_data = __in_e_flits_data;
  logic [31:0] wx_data;
  assign wx_data = __in_w_flits_data;
  logic [1:0] lx_dx;
  assign lx_dx = lx_data[1:0];
  logic [1:0] lx_dy;
  assign lx_dy = lx_data[3:2];
  logic [1:0] nx_dx;
  assign nx_dx = nx_data[1:0];
  logic [1:0] nx_dy;
  assign nx_dy = nx_data[3:2];
  logic [1:0] sx_dx;
  assign sx_dx = sx_data[1:0];
  logic [1:0] sx_dy;
  assign sx_dy = sx_data[3:2];
  logic [1:0] ex_dx;
  assign ex_dx = ex_data[1:0];
  logic [1:0] ex_dy;
  assign ex_dy = ex_data[3:2];
  logic [1:0] wx_dx;
  assign wx_dx = wx_data[1:0];
  logic [1:0] wx_dy;
  assign wx_dy = wx_data[3:2];
  logic [1:0] mx;
  assign mx = X;
  logic [1:0] my;
  assign my = Y;
  logic [2:0] route_local;
  assign route_local = lx_dx > mx ? 3'd3 : lx_dx < mx ? 3'd4 : lx_dy > my ? 3'd1 : lx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_n;
  assign route_n = nx_dx > mx ? 3'd3 : nx_dx < mx ? 3'd4 : nx_dy > my ? 3'd1 : nx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_s;
  assign route_s = sx_dx > mx ? 3'd3 : sx_dx < mx ? 3'd4 : sx_dy > my ? 3'd1 : sx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_e;
  assign route_e = ex_dx > mx ? 3'd3 : ex_dx < mx ? 3'd4 : ex_dy > my ? 3'd1 : ex_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_w;
  assign route_w = wx_dx > mx ? 3'd3 : wx_dx < mx ? 3'd4 : wx_dy > my ? 3'd1 : wx_dy < my ? 3'd2 : 3'd0;
  logic want_local_local;
  assign want_local_local = __in_local_flits_valid && route_local == 3'd0;
  logic want_n_local;
  assign want_n_local = __in_n_flits_valid && route_n == 3'd0;
  logic want_s_local;
  assign want_s_local = __in_s_flits_valid && route_s == 3'd0;
  logic want_e_local;
  assign want_e_local = __in_e_flits_valid && route_e == 3'd0;
  logic want_w_local;
  assign want_w_local = __in_w_flits_valid && route_w == 3'd0;
  logic want_local_n;
  assign want_local_n = __in_local_flits_valid && route_local == 3'd1;
  logic want_n_n;
  assign want_n_n = __in_n_flits_valid && route_n == 3'd1;
  logic want_s_n;
  assign want_s_n = __in_s_flits_valid && route_s == 3'd1;
  logic want_e_n;
  assign want_e_n = __in_e_flits_valid && route_e == 3'd1;
  logic want_w_n;
  assign want_w_n = __in_w_flits_valid && route_w == 3'd1;
  logic want_local_s;
  assign want_local_s = __in_local_flits_valid && route_local == 3'd2;
  logic want_n_s;
  assign want_n_s = __in_n_flits_valid && route_n == 3'd2;
  logic want_s_s;
  assign want_s_s = __in_s_flits_valid && route_s == 3'd2;
  logic want_e_s;
  assign want_e_s = __in_e_flits_valid && route_e == 3'd2;
  logic want_w_s;
  assign want_w_s = __in_w_flits_valid && route_w == 3'd2;
  logic want_local_e;
  assign want_local_e = __in_local_flits_valid && route_local == 3'd3;
  logic want_n_e;
  assign want_n_e = __in_n_flits_valid && route_n == 3'd3;
  logic want_s_e;
  assign want_s_e = __in_s_flits_valid && route_s == 3'd3;
  logic want_e_e;
  assign want_e_e = __in_e_flits_valid && route_e == 3'd3;
  logic want_w_e;
  assign want_w_e = __in_w_flits_valid && route_w == 3'd3;
  logic want_local_w;
  assign want_local_w = __in_local_flits_valid && route_local == 3'd4;
  logic want_n_w;
  assign want_n_w = __in_n_flits_valid && route_n == 3'd4;
  logic want_s_w;
  assign want_s_w = __in_s_flits_valid && route_s == 3'd4;
  logic want_e_w;
  assign want_e_w = __in_e_flits_valid && route_e == 3'd4;
  logic want_w_w;
  assign want_w_w = __in_w_flits_valid && route_w == 3'd4;
  logic [2:0] pick_local;
  assign pick_local = __out_local_flits_can_send && want_local_local ? 3'd1 : __out_local_flits_can_send && want_n_local ? 3'd2 : __out_local_flits_can_send && want_s_local ? 3'd3 : __out_local_flits_can_send && want_e_local ? 3'd4 : __out_local_flits_can_send && want_w_local ? 3'd5 : 3'd0;
  logic [2:0] pick_n;
  assign pick_n = __out_n_flits_can_send && want_local_n ? 3'd1 : __out_n_flits_can_send && want_n_n ? 3'd2 : __out_n_flits_can_send && want_s_n ? 3'd3 : __out_n_flits_can_send && want_e_n ? 3'd4 : __out_n_flits_can_send && want_w_n ? 3'd5 : 3'd0;
  logic [2:0] pick_s;
  assign pick_s = __out_s_flits_can_send && want_local_s ? 3'd1 : __out_s_flits_can_send && want_n_s ? 3'd2 : __out_s_flits_can_send && want_s_s ? 3'd3 : __out_s_flits_can_send && want_e_s ? 3'd4 : __out_s_flits_can_send && want_w_s ? 3'd5 : 3'd0;
  logic [2:0] pick_e;
  assign pick_e = __out_e_flits_can_send && want_local_e ? 3'd1 : __out_e_flits_can_send && want_n_e ? 3'd2 : __out_e_flits_can_send && want_s_e ? 3'd3 : __out_e_flits_can_send && want_e_e ? 3'd4 : __out_e_flits_can_send && want_w_e ? 3'd5 : 3'd0;
  logic [2:0] pick_w;
  assign pick_w = __out_w_flits_can_send && want_local_w ? 3'd1 : __out_w_flits_can_send && want_n_w ? 3'd2 : __out_w_flits_can_send && want_s_w ? 3'd3 : __out_w_flits_can_send && want_e_w ? 3'd4 : __out_w_flits_can_send && want_w_w ? 3'd5 : 3'd0;
  logic served_local;
  assign served_local = pick_local == 3'd1 || pick_n == 3'd1 || pick_s == 3'd1 || pick_e == 3'd1 || pick_w == 3'd1;
  logic served_n;
  assign served_n = pick_local == 3'd2 || pick_n == 3'd2 || pick_s == 3'd2 || pick_e == 3'd2 || pick_w == 3'd2;
  logic served_s;
  assign served_s = pick_local == 3'd3 || pick_n == 3'd3 || pick_s == 3'd3 || pick_e == 3'd3 || pick_w == 3'd3;
  logic served_e;
  assign served_e = pick_local == 3'd4 || pick_n == 3'd4 || pick_s == 3'd4 || pick_e == 3'd4 || pick_w == 3'd4;
  logic served_w;
  assign served_w = pick_local == 3'd5 || pick_n == 3'd5 || pick_s == 3'd5 || pick_e == 3'd5 || pick_w == 3'd5;
  assign out_local_flits_send_valid = pick_local != 3'd0;
  assign out_local_flits_send_data = pick_local == 3'd1 ? lx_data : pick_local == 3'd2 ? nx_data : pick_local == 3'd3 ? sx_data : pick_local == 3'd4 ? ex_data : wx_data;
  assign out_n_flits_send_valid = pick_n != 3'd0;
  assign out_n_flits_send_data = pick_n == 3'd1 ? lx_data : pick_n == 3'd2 ? nx_data : pick_n == 3'd3 ? sx_data : pick_n == 3'd4 ? ex_data : wx_data;
  assign out_s_flits_send_valid = pick_s != 3'd0;
  assign out_s_flits_send_data = pick_s == 3'd1 ? lx_data : pick_s == 3'd2 ? nx_data : pick_s == 3'd3 ? sx_data : pick_s == 3'd4 ? ex_data : wx_data;
  assign out_e_flits_send_valid = pick_e != 3'd0;
  assign out_e_flits_send_data = pick_e == 3'd1 ? lx_data : pick_e == 3'd2 ? nx_data : pick_e == 3'd3 ? sx_data : pick_e == 3'd4 ? ex_data : wx_data;
  assign out_w_flits_send_valid = pick_w != 3'd0;
  assign out_w_flits_send_data = pick_w == 3'd1 ? lx_data : pick_w == 3'd2 ? nx_data : pick_w == 3'd3 ? sx_data : pick_w == 3'd4 ? ex_data : wx_data;
  assign in_local_flits_credit_return = served_local;
  assign in_n_flits_credit_return = served_n;
  assign in_s_flits_credit_return = served_s;
  assign in_e_flits_credit_return = served_e;
  assign in_w_flits_credit_return = served_w;
  
  // Auto-generated credit_channel state (PR #3b-ii, sender side)
  logic [$clog2((4) + 1) - 1:0] __out_local_flits_credit;
  wire  __out_local_flits_can_send = __out_local_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_local_flits_credit <= 4;
    end else begin
      if (out_local_flits_send_valid && !out_local_flits_credit_return) __out_local_flits_credit <= __out_local_flits_credit - 1;
      else if (out_local_flits_credit_return && !out_local_flits_send_valid) __out_local_flits_credit <= __out_local_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_n_flits_credit;
  wire  __out_n_flits_can_send = __out_n_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_n_flits_credit <= 4;
    end else begin
      if (out_n_flits_send_valid && !out_n_flits_credit_return) __out_n_flits_credit <= __out_n_flits_credit - 1;
      else if (out_n_flits_credit_return && !out_n_flits_send_valid) __out_n_flits_credit <= __out_n_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_s_flits_credit;
  wire  __out_s_flits_can_send = __out_s_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_s_flits_credit <= 4;
    end else begin
      if (out_s_flits_send_valid && !out_s_flits_credit_return) __out_s_flits_credit <= __out_s_flits_credit - 1;
      else if (out_s_flits_credit_return && !out_s_flits_send_valid) __out_s_flits_credit <= __out_s_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_e_flits_credit;
  wire  __out_e_flits_can_send = __out_e_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_e_flits_credit <= 4;
    end else begin
      if (out_e_flits_send_valid && !out_e_flits_credit_return) __out_e_flits_credit <= __out_e_flits_credit - 1;
      else if (out_e_flits_credit_return && !out_e_flits_send_valid) __out_e_flits_credit <= __out_e_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_w_flits_credit;
  wire  __out_w_flits_can_send = __out_w_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_w_flits_credit <= 4;
    end else begin
      if (out_w_flits_send_valid && !out_w_flits_credit_return) __out_w_flits_credit <= __out_w_flits_credit - 1;
      else if (out_w_flits_credit_return && !out_w_flits_send_valid) __out_w_flits_credit <= __out_w_flits_credit + 1;
    end
  end
  
  // Auto-generated credit_channel target-side FIFO (PR #3b-iii)
  logic [(32) - 1:0] __in_local_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_local_flits_occ;
  wire  __in_local_flits_valid = __in_local_flits_occ != 0;
  wire [(32) - 1:0] __in_local_flits_data = __in_local_flits_buf[__in_local_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_local_flits_head <= 0;
      __in_local_flits_tail <= 0;
      __in_local_flits_occ  <= 0;
    end else begin
      if (in_local_flits_send_valid) begin
        __in_local_flits_buf[__in_local_flits_tail] <= in_local_flits_send_data;
        __in_local_flits_tail <= (__in_local_flits_tail + 1) % (4);
      end
      if ((in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_head <= (__in_local_flits_head + 1) % (4);
      if (in_local_flits_send_valid && !(in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ + 1;
      else if (!in_local_flits_send_valid &&  (in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_n_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_n_flits_occ;
  wire  __in_n_flits_valid = __in_n_flits_occ != 0;
  wire [(32) - 1:0] __in_n_flits_data = __in_n_flits_buf[__in_n_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_n_flits_head <= 0;
      __in_n_flits_tail <= 0;
      __in_n_flits_occ  <= 0;
    end else begin
      if (in_n_flits_send_valid) begin
        __in_n_flits_buf[__in_n_flits_tail] <= in_n_flits_send_data;
        __in_n_flits_tail <= (__in_n_flits_tail + 1) % (4);
      end
      if ((in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_head <= (__in_n_flits_head + 1) % (4);
      if (in_n_flits_send_valid && !(in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ + 1;
      else if (!in_n_flits_send_valid &&  (in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_s_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_s_flits_occ;
  wire  __in_s_flits_valid = __in_s_flits_occ != 0;
  wire [(32) - 1:0] __in_s_flits_data = __in_s_flits_buf[__in_s_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_s_flits_head <= 0;
      __in_s_flits_tail <= 0;
      __in_s_flits_occ  <= 0;
    end else begin
      if (in_s_flits_send_valid) begin
        __in_s_flits_buf[__in_s_flits_tail] <= in_s_flits_send_data;
        __in_s_flits_tail <= (__in_s_flits_tail + 1) % (4);
      end
      if ((in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_head <= (__in_s_flits_head + 1) % (4);
      if (in_s_flits_send_valid && !(in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ + 1;
      else if (!in_s_flits_send_valid &&  (in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_e_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_e_flits_occ;
  wire  __in_e_flits_valid = __in_e_flits_occ != 0;
  wire [(32) - 1:0] __in_e_flits_data = __in_e_flits_buf[__in_e_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_e_flits_head <= 0;
      __in_e_flits_tail <= 0;
      __in_e_flits_occ  <= 0;
    end else begin
      if (in_e_flits_send_valid) begin
        __in_e_flits_buf[__in_e_flits_tail] <= in_e_flits_send_data;
        __in_e_flits_tail <= (__in_e_flits_tail + 1) % (4);
      end
      if ((in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_head <= (__in_e_flits_head + 1) % (4);
      if (in_e_flits_send_valid && !(in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ + 1;
      else if (!in_e_flits_send_valid &&  (in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_w_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_w_flits_occ;
  wire  __in_w_flits_valid = __in_w_flits_occ != 0;
  wire [(32) - 1:0] __in_w_flits_data = __in_w_flits_buf[__in_w_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_w_flits_head <= 0;
      __in_w_flits_tail <= 0;
      __in_w_flits_occ  <= 0;
    end else begin
      if (in_w_flits_send_valid) begin
        __in_w_flits_buf[__in_w_flits_tail] <= in_w_flits_send_data;
        __in_w_flits_tail <= (__in_w_flits_tail + 1) % (4);
      end
      if ((in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_head <= (__in_w_flits_head + 1) % (4);
      if (in_w_flits_send_valid && !(in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ + 1;
      else if (!in_w_flits_send_valid &&  (in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ - 1;
    end
  end
  
  // synopsys translate_off
  // Auto-generated credit_channel protocol assertions (Tier 2)
  _auto_cc_out_local_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_local_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_2_Y_2._auto_cc_out_local_flits_credit_bounds");
  _auto_cc_out_local_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_local_flits_send_valid |-> __out_local_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_2_Y_2._auto_cc_out_local_flits_send_requires_credit");
  _auto_cc_out_n_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_n_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_2_Y_2._auto_cc_out_n_flits_credit_bounds");
  _auto_cc_out_n_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_n_flits_send_valid |-> __out_n_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_2_Y_2._auto_cc_out_n_flits_send_requires_credit");
  _auto_cc_out_s_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_s_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_2_Y_2._auto_cc_out_s_flits_credit_bounds");
  _auto_cc_out_s_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_s_flits_send_valid |-> __out_s_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_2_Y_2._auto_cc_out_s_flits_send_requires_credit");
  _auto_cc_out_e_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_e_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_2_Y_2._auto_cc_out_e_flits_credit_bounds");
  _auto_cc_out_e_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_e_flits_send_valid |-> __out_e_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_2_Y_2._auto_cc_out_e_flits_send_requires_credit");
  _auto_cc_out_w_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_w_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_2_Y_2._auto_cc_out_w_flits_credit_bounds");
  _auto_cc_out_w_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_w_flits_send_valid |-> __out_w_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_2_Y_2._auto_cc_out_w_flits_send_requires_credit");
  _auto_cc_in_local_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_local_flits_credit_return |-> __in_local_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_2_Y_2._auto_cc_in_local_flits_credit_return_requires_buffered");
  _auto_cc_in_n_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_n_flits_credit_return |-> __in_n_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_2_Y_2._auto_cc_in_n_flits_credit_return_requires_buffered");
  _auto_cc_in_s_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_s_flits_credit_return |-> __in_s_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_2_Y_2._auto_cc_in_s_flits_credit_return_requires_buffered");
  _auto_cc_in_e_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_e_flits_credit_return |-> __in_e_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_2_Y_2._auto_cc_in_e_flits_credit_return_requires_buffered");
  _auto_cc_in_w_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_w_flits_credit_return |-> __in_w_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_2_Y_2._auto_cc_in_w_flits_credit_return_requires_buffered");
  // synopsys translate_on

endmodule

module Router__X_3_Y_2 #(
  parameter int X = 3,
  parameter int Y = 2
) (
  input logic clk,
  input logic rst,
  input logic in_local_flits_send_valid,
  input logic [31:0] in_local_flits_send_data,
  output logic in_local_flits_credit_return,
  input logic in_n_flits_send_valid,
  input logic [31:0] in_n_flits_send_data,
  output logic in_n_flits_credit_return,
  input logic in_s_flits_send_valid,
  input logic [31:0] in_s_flits_send_data,
  output logic in_s_flits_credit_return,
  input logic in_e_flits_send_valid,
  input logic [31:0] in_e_flits_send_data,
  output logic in_e_flits_credit_return,
  input logic in_w_flits_send_valid,
  input logic [31:0] in_w_flits_send_data,
  output logic in_w_flits_credit_return,
  output logic out_local_flits_send_valid,
  output logic [31:0] out_local_flits_send_data,
  input logic out_local_flits_credit_return,
  output logic out_n_flits_send_valid,
  output logic [31:0] out_n_flits_send_data,
  input logic out_n_flits_credit_return,
  output logic out_s_flits_send_valid,
  output logic [31:0] out_s_flits_send_data,
  input logic out_s_flits_credit_return,
  output logic out_e_flits_send_valid,
  output logic [31:0] out_e_flits_send_data,
  input logic out_e_flits_credit_return,
  output logic out_w_flits_send_valid,
  output logic [31:0] out_w_flits_send_data,
  input logic out_w_flits_credit_return
);

  logic [31:0] lx_data;
  assign lx_data = __in_local_flits_data;
  logic [31:0] nx_data;
  assign nx_data = __in_n_flits_data;
  logic [31:0] sx_data;
  assign sx_data = __in_s_flits_data;
  logic [31:0] ex_data;
  assign ex_data = __in_e_flits_data;
  logic [31:0] wx_data;
  assign wx_data = __in_w_flits_data;
  logic [1:0] lx_dx;
  assign lx_dx = lx_data[1:0];
  logic [1:0] lx_dy;
  assign lx_dy = lx_data[3:2];
  logic [1:0] nx_dx;
  assign nx_dx = nx_data[1:0];
  logic [1:0] nx_dy;
  assign nx_dy = nx_data[3:2];
  logic [1:0] sx_dx;
  assign sx_dx = sx_data[1:0];
  logic [1:0] sx_dy;
  assign sx_dy = sx_data[3:2];
  logic [1:0] ex_dx;
  assign ex_dx = ex_data[1:0];
  logic [1:0] ex_dy;
  assign ex_dy = ex_data[3:2];
  logic [1:0] wx_dx;
  assign wx_dx = wx_data[1:0];
  logic [1:0] wx_dy;
  assign wx_dy = wx_data[3:2];
  logic [1:0] mx;
  assign mx = X;
  logic [1:0] my;
  assign my = Y;
  logic [2:0] route_local;
  assign route_local = lx_dx > mx ? 3'd3 : lx_dx < mx ? 3'd4 : lx_dy > my ? 3'd1 : lx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_n;
  assign route_n = nx_dx > mx ? 3'd3 : nx_dx < mx ? 3'd4 : nx_dy > my ? 3'd1 : nx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_s;
  assign route_s = sx_dx > mx ? 3'd3 : sx_dx < mx ? 3'd4 : sx_dy > my ? 3'd1 : sx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_e;
  assign route_e = ex_dx > mx ? 3'd3 : ex_dx < mx ? 3'd4 : ex_dy > my ? 3'd1 : ex_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_w;
  assign route_w = wx_dx > mx ? 3'd3 : wx_dx < mx ? 3'd4 : wx_dy > my ? 3'd1 : wx_dy < my ? 3'd2 : 3'd0;
  logic want_local_local;
  assign want_local_local = __in_local_flits_valid && route_local == 3'd0;
  logic want_n_local;
  assign want_n_local = __in_n_flits_valid && route_n == 3'd0;
  logic want_s_local;
  assign want_s_local = __in_s_flits_valid && route_s == 3'd0;
  logic want_e_local;
  assign want_e_local = __in_e_flits_valid && route_e == 3'd0;
  logic want_w_local;
  assign want_w_local = __in_w_flits_valid && route_w == 3'd0;
  logic want_local_n;
  assign want_local_n = __in_local_flits_valid && route_local == 3'd1;
  logic want_n_n;
  assign want_n_n = __in_n_flits_valid && route_n == 3'd1;
  logic want_s_n;
  assign want_s_n = __in_s_flits_valid && route_s == 3'd1;
  logic want_e_n;
  assign want_e_n = __in_e_flits_valid && route_e == 3'd1;
  logic want_w_n;
  assign want_w_n = __in_w_flits_valid && route_w == 3'd1;
  logic want_local_s;
  assign want_local_s = __in_local_flits_valid && route_local == 3'd2;
  logic want_n_s;
  assign want_n_s = __in_n_flits_valid && route_n == 3'd2;
  logic want_s_s;
  assign want_s_s = __in_s_flits_valid && route_s == 3'd2;
  logic want_e_s;
  assign want_e_s = __in_e_flits_valid && route_e == 3'd2;
  logic want_w_s;
  assign want_w_s = __in_w_flits_valid && route_w == 3'd2;
  logic want_local_e;
  assign want_local_e = __in_local_flits_valid && route_local == 3'd3;
  logic want_n_e;
  assign want_n_e = __in_n_flits_valid && route_n == 3'd3;
  logic want_s_e;
  assign want_s_e = __in_s_flits_valid && route_s == 3'd3;
  logic want_e_e;
  assign want_e_e = __in_e_flits_valid && route_e == 3'd3;
  logic want_w_e;
  assign want_w_e = __in_w_flits_valid && route_w == 3'd3;
  logic want_local_w;
  assign want_local_w = __in_local_flits_valid && route_local == 3'd4;
  logic want_n_w;
  assign want_n_w = __in_n_flits_valid && route_n == 3'd4;
  logic want_s_w;
  assign want_s_w = __in_s_flits_valid && route_s == 3'd4;
  logic want_e_w;
  assign want_e_w = __in_e_flits_valid && route_e == 3'd4;
  logic want_w_w;
  assign want_w_w = __in_w_flits_valid && route_w == 3'd4;
  logic [2:0] pick_local;
  assign pick_local = __out_local_flits_can_send && want_local_local ? 3'd1 : __out_local_flits_can_send && want_n_local ? 3'd2 : __out_local_flits_can_send && want_s_local ? 3'd3 : __out_local_flits_can_send && want_e_local ? 3'd4 : __out_local_flits_can_send && want_w_local ? 3'd5 : 3'd0;
  logic [2:0] pick_n;
  assign pick_n = __out_n_flits_can_send && want_local_n ? 3'd1 : __out_n_flits_can_send && want_n_n ? 3'd2 : __out_n_flits_can_send && want_s_n ? 3'd3 : __out_n_flits_can_send && want_e_n ? 3'd4 : __out_n_flits_can_send && want_w_n ? 3'd5 : 3'd0;
  logic [2:0] pick_s;
  assign pick_s = __out_s_flits_can_send && want_local_s ? 3'd1 : __out_s_flits_can_send && want_n_s ? 3'd2 : __out_s_flits_can_send && want_s_s ? 3'd3 : __out_s_flits_can_send && want_e_s ? 3'd4 : __out_s_flits_can_send && want_w_s ? 3'd5 : 3'd0;
  logic [2:0] pick_e;
  assign pick_e = __out_e_flits_can_send && want_local_e ? 3'd1 : __out_e_flits_can_send && want_n_e ? 3'd2 : __out_e_flits_can_send && want_s_e ? 3'd3 : __out_e_flits_can_send && want_e_e ? 3'd4 : __out_e_flits_can_send && want_w_e ? 3'd5 : 3'd0;
  logic [2:0] pick_w;
  assign pick_w = __out_w_flits_can_send && want_local_w ? 3'd1 : __out_w_flits_can_send && want_n_w ? 3'd2 : __out_w_flits_can_send && want_s_w ? 3'd3 : __out_w_flits_can_send && want_e_w ? 3'd4 : __out_w_flits_can_send && want_w_w ? 3'd5 : 3'd0;
  logic served_local;
  assign served_local = pick_local == 3'd1 || pick_n == 3'd1 || pick_s == 3'd1 || pick_e == 3'd1 || pick_w == 3'd1;
  logic served_n;
  assign served_n = pick_local == 3'd2 || pick_n == 3'd2 || pick_s == 3'd2 || pick_e == 3'd2 || pick_w == 3'd2;
  logic served_s;
  assign served_s = pick_local == 3'd3 || pick_n == 3'd3 || pick_s == 3'd3 || pick_e == 3'd3 || pick_w == 3'd3;
  logic served_e;
  assign served_e = pick_local == 3'd4 || pick_n == 3'd4 || pick_s == 3'd4 || pick_e == 3'd4 || pick_w == 3'd4;
  logic served_w;
  assign served_w = pick_local == 3'd5 || pick_n == 3'd5 || pick_s == 3'd5 || pick_e == 3'd5 || pick_w == 3'd5;
  assign out_local_flits_send_valid = pick_local != 3'd0;
  assign out_local_flits_send_data = pick_local == 3'd1 ? lx_data : pick_local == 3'd2 ? nx_data : pick_local == 3'd3 ? sx_data : pick_local == 3'd4 ? ex_data : wx_data;
  assign out_n_flits_send_valid = pick_n != 3'd0;
  assign out_n_flits_send_data = pick_n == 3'd1 ? lx_data : pick_n == 3'd2 ? nx_data : pick_n == 3'd3 ? sx_data : pick_n == 3'd4 ? ex_data : wx_data;
  assign out_s_flits_send_valid = pick_s != 3'd0;
  assign out_s_flits_send_data = pick_s == 3'd1 ? lx_data : pick_s == 3'd2 ? nx_data : pick_s == 3'd3 ? sx_data : pick_s == 3'd4 ? ex_data : wx_data;
  assign out_e_flits_send_valid = pick_e != 3'd0;
  assign out_e_flits_send_data = pick_e == 3'd1 ? lx_data : pick_e == 3'd2 ? nx_data : pick_e == 3'd3 ? sx_data : pick_e == 3'd4 ? ex_data : wx_data;
  assign out_w_flits_send_valid = pick_w != 3'd0;
  assign out_w_flits_send_data = pick_w == 3'd1 ? lx_data : pick_w == 3'd2 ? nx_data : pick_w == 3'd3 ? sx_data : pick_w == 3'd4 ? ex_data : wx_data;
  assign in_local_flits_credit_return = served_local;
  assign in_n_flits_credit_return = served_n;
  assign in_s_flits_credit_return = served_s;
  assign in_e_flits_credit_return = served_e;
  assign in_w_flits_credit_return = served_w;
  
  // Auto-generated credit_channel state (PR #3b-ii, sender side)
  logic [$clog2((4) + 1) - 1:0] __out_local_flits_credit;
  wire  __out_local_flits_can_send = __out_local_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_local_flits_credit <= 4;
    end else begin
      if (out_local_flits_send_valid && !out_local_flits_credit_return) __out_local_flits_credit <= __out_local_flits_credit - 1;
      else if (out_local_flits_credit_return && !out_local_flits_send_valid) __out_local_flits_credit <= __out_local_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_n_flits_credit;
  wire  __out_n_flits_can_send = __out_n_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_n_flits_credit <= 4;
    end else begin
      if (out_n_flits_send_valid && !out_n_flits_credit_return) __out_n_flits_credit <= __out_n_flits_credit - 1;
      else if (out_n_flits_credit_return && !out_n_flits_send_valid) __out_n_flits_credit <= __out_n_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_s_flits_credit;
  wire  __out_s_flits_can_send = __out_s_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_s_flits_credit <= 4;
    end else begin
      if (out_s_flits_send_valid && !out_s_flits_credit_return) __out_s_flits_credit <= __out_s_flits_credit - 1;
      else if (out_s_flits_credit_return && !out_s_flits_send_valid) __out_s_flits_credit <= __out_s_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_e_flits_credit;
  wire  __out_e_flits_can_send = __out_e_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_e_flits_credit <= 4;
    end else begin
      if (out_e_flits_send_valid && !out_e_flits_credit_return) __out_e_flits_credit <= __out_e_flits_credit - 1;
      else if (out_e_flits_credit_return && !out_e_flits_send_valid) __out_e_flits_credit <= __out_e_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_w_flits_credit;
  wire  __out_w_flits_can_send = __out_w_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_w_flits_credit <= 4;
    end else begin
      if (out_w_flits_send_valid && !out_w_flits_credit_return) __out_w_flits_credit <= __out_w_flits_credit - 1;
      else if (out_w_flits_credit_return && !out_w_flits_send_valid) __out_w_flits_credit <= __out_w_flits_credit + 1;
    end
  end
  
  // Auto-generated credit_channel target-side FIFO (PR #3b-iii)
  logic [(32) - 1:0] __in_local_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_local_flits_occ;
  wire  __in_local_flits_valid = __in_local_flits_occ != 0;
  wire [(32) - 1:0] __in_local_flits_data = __in_local_flits_buf[__in_local_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_local_flits_head <= 0;
      __in_local_flits_tail <= 0;
      __in_local_flits_occ  <= 0;
    end else begin
      if (in_local_flits_send_valid) begin
        __in_local_flits_buf[__in_local_flits_tail] <= in_local_flits_send_data;
        __in_local_flits_tail <= (__in_local_flits_tail + 1) % (4);
      end
      if ((in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_head <= (__in_local_flits_head + 1) % (4);
      if (in_local_flits_send_valid && !(in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ + 1;
      else if (!in_local_flits_send_valid &&  (in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_n_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_n_flits_occ;
  wire  __in_n_flits_valid = __in_n_flits_occ != 0;
  wire [(32) - 1:0] __in_n_flits_data = __in_n_flits_buf[__in_n_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_n_flits_head <= 0;
      __in_n_flits_tail <= 0;
      __in_n_flits_occ  <= 0;
    end else begin
      if (in_n_flits_send_valid) begin
        __in_n_flits_buf[__in_n_flits_tail] <= in_n_flits_send_data;
        __in_n_flits_tail <= (__in_n_flits_tail + 1) % (4);
      end
      if ((in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_head <= (__in_n_flits_head + 1) % (4);
      if (in_n_flits_send_valid && !(in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ + 1;
      else if (!in_n_flits_send_valid &&  (in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_s_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_s_flits_occ;
  wire  __in_s_flits_valid = __in_s_flits_occ != 0;
  wire [(32) - 1:0] __in_s_flits_data = __in_s_flits_buf[__in_s_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_s_flits_head <= 0;
      __in_s_flits_tail <= 0;
      __in_s_flits_occ  <= 0;
    end else begin
      if (in_s_flits_send_valid) begin
        __in_s_flits_buf[__in_s_flits_tail] <= in_s_flits_send_data;
        __in_s_flits_tail <= (__in_s_flits_tail + 1) % (4);
      end
      if ((in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_head <= (__in_s_flits_head + 1) % (4);
      if (in_s_flits_send_valid && !(in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ + 1;
      else if (!in_s_flits_send_valid &&  (in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_e_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_e_flits_occ;
  wire  __in_e_flits_valid = __in_e_flits_occ != 0;
  wire [(32) - 1:0] __in_e_flits_data = __in_e_flits_buf[__in_e_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_e_flits_head <= 0;
      __in_e_flits_tail <= 0;
      __in_e_flits_occ  <= 0;
    end else begin
      if (in_e_flits_send_valid) begin
        __in_e_flits_buf[__in_e_flits_tail] <= in_e_flits_send_data;
        __in_e_flits_tail <= (__in_e_flits_tail + 1) % (4);
      end
      if ((in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_head <= (__in_e_flits_head + 1) % (4);
      if (in_e_flits_send_valid && !(in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ + 1;
      else if (!in_e_flits_send_valid &&  (in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_w_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_w_flits_occ;
  wire  __in_w_flits_valid = __in_w_flits_occ != 0;
  wire [(32) - 1:0] __in_w_flits_data = __in_w_flits_buf[__in_w_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_w_flits_head <= 0;
      __in_w_flits_tail <= 0;
      __in_w_flits_occ  <= 0;
    end else begin
      if (in_w_flits_send_valid) begin
        __in_w_flits_buf[__in_w_flits_tail] <= in_w_flits_send_data;
        __in_w_flits_tail <= (__in_w_flits_tail + 1) % (4);
      end
      if ((in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_head <= (__in_w_flits_head + 1) % (4);
      if (in_w_flits_send_valid && !(in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ + 1;
      else if (!in_w_flits_send_valid &&  (in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ - 1;
    end
  end
  
  // synopsys translate_off
  // Auto-generated credit_channel protocol assertions (Tier 2)
  _auto_cc_out_local_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_local_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_3_Y_2._auto_cc_out_local_flits_credit_bounds");
  _auto_cc_out_local_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_local_flits_send_valid |-> __out_local_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_3_Y_2._auto_cc_out_local_flits_send_requires_credit");
  _auto_cc_out_n_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_n_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_3_Y_2._auto_cc_out_n_flits_credit_bounds");
  _auto_cc_out_n_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_n_flits_send_valid |-> __out_n_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_3_Y_2._auto_cc_out_n_flits_send_requires_credit");
  _auto_cc_out_s_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_s_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_3_Y_2._auto_cc_out_s_flits_credit_bounds");
  _auto_cc_out_s_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_s_flits_send_valid |-> __out_s_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_3_Y_2._auto_cc_out_s_flits_send_requires_credit");
  _auto_cc_out_e_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_e_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_3_Y_2._auto_cc_out_e_flits_credit_bounds");
  _auto_cc_out_e_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_e_flits_send_valid |-> __out_e_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_3_Y_2._auto_cc_out_e_flits_send_requires_credit");
  _auto_cc_out_w_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_w_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_3_Y_2._auto_cc_out_w_flits_credit_bounds");
  _auto_cc_out_w_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_w_flits_send_valid |-> __out_w_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_3_Y_2._auto_cc_out_w_flits_send_requires_credit");
  _auto_cc_in_local_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_local_flits_credit_return |-> __in_local_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_3_Y_2._auto_cc_in_local_flits_credit_return_requires_buffered");
  _auto_cc_in_n_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_n_flits_credit_return |-> __in_n_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_3_Y_2._auto_cc_in_n_flits_credit_return_requires_buffered");
  _auto_cc_in_s_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_s_flits_credit_return |-> __in_s_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_3_Y_2._auto_cc_in_s_flits_credit_return_requires_buffered");
  _auto_cc_in_e_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_e_flits_credit_return |-> __in_e_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_3_Y_2._auto_cc_in_e_flits_credit_return_requires_buffered");
  _auto_cc_in_w_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_w_flits_credit_return |-> __in_w_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_3_Y_2._auto_cc_in_w_flits_credit_return_requires_buffered");
  // synopsys translate_on

endmodule

module Router__X_0_Y_3 #(
  parameter int X = 0,
  parameter int Y = 3
) (
  input logic clk,
  input logic rst,
  input logic in_local_flits_send_valid,
  input logic [31:0] in_local_flits_send_data,
  output logic in_local_flits_credit_return,
  input logic in_n_flits_send_valid,
  input logic [31:0] in_n_flits_send_data,
  output logic in_n_flits_credit_return,
  input logic in_s_flits_send_valid,
  input logic [31:0] in_s_flits_send_data,
  output logic in_s_flits_credit_return,
  input logic in_e_flits_send_valid,
  input logic [31:0] in_e_flits_send_data,
  output logic in_e_flits_credit_return,
  input logic in_w_flits_send_valid,
  input logic [31:0] in_w_flits_send_data,
  output logic in_w_flits_credit_return,
  output logic out_local_flits_send_valid,
  output logic [31:0] out_local_flits_send_data,
  input logic out_local_flits_credit_return,
  output logic out_n_flits_send_valid,
  output logic [31:0] out_n_flits_send_data,
  input logic out_n_flits_credit_return,
  output logic out_s_flits_send_valid,
  output logic [31:0] out_s_flits_send_data,
  input logic out_s_flits_credit_return,
  output logic out_e_flits_send_valid,
  output logic [31:0] out_e_flits_send_data,
  input logic out_e_flits_credit_return,
  output logic out_w_flits_send_valid,
  output logic [31:0] out_w_flits_send_data,
  input logic out_w_flits_credit_return
);

  logic [31:0] lx_data;
  assign lx_data = __in_local_flits_data;
  logic [31:0] nx_data;
  assign nx_data = __in_n_flits_data;
  logic [31:0] sx_data;
  assign sx_data = __in_s_flits_data;
  logic [31:0] ex_data;
  assign ex_data = __in_e_flits_data;
  logic [31:0] wx_data;
  assign wx_data = __in_w_flits_data;
  logic [1:0] lx_dx;
  assign lx_dx = lx_data[1:0];
  logic [1:0] lx_dy;
  assign lx_dy = lx_data[3:2];
  logic [1:0] nx_dx;
  assign nx_dx = nx_data[1:0];
  logic [1:0] nx_dy;
  assign nx_dy = nx_data[3:2];
  logic [1:0] sx_dx;
  assign sx_dx = sx_data[1:0];
  logic [1:0] sx_dy;
  assign sx_dy = sx_data[3:2];
  logic [1:0] ex_dx;
  assign ex_dx = ex_data[1:0];
  logic [1:0] ex_dy;
  assign ex_dy = ex_data[3:2];
  logic [1:0] wx_dx;
  assign wx_dx = wx_data[1:0];
  logic [1:0] wx_dy;
  assign wx_dy = wx_data[3:2];
  logic [1:0] mx;
  assign mx = X;
  logic [1:0] my;
  assign my = Y;
  logic [2:0] route_local;
  assign route_local = lx_dx > mx ? 3'd3 : lx_dx < mx ? 3'd4 : lx_dy > my ? 3'd1 : lx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_n;
  assign route_n = nx_dx > mx ? 3'd3 : nx_dx < mx ? 3'd4 : nx_dy > my ? 3'd1 : nx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_s;
  assign route_s = sx_dx > mx ? 3'd3 : sx_dx < mx ? 3'd4 : sx_dy > my ? 3'd1 : sx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_e;
  assign route_e = ex_dx > mx ? 3'd3 : ex_dx < mx ? 3'd4 : ex_dy > my ? 3'd1 : ex_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_w;
  assign route_w = wx_dx > mx ? 3'd3 : wx_dx < mx ? 3'd4 : wx_dy > my ? 3'd1 : wx_dy < my ? 3'd2 : 3'd0;
  logic want_local_local;
  assign want_local_local = __in_local_flits_valid && route_local == 3'd0;
  logic want_n_local;
  assign want_n_local = __in_n_flits_valid && route_n == 3'd0;
  logic want_s_local;
  assign want_s_local = __in_s_flits_valid && route_s == 3'd0;
  logic want_e_local;
  assign want_e_local = __in_e_flits_valid && route_e == 3'd0;
  logic want_w_local;
  assign want_w_local = __in_w_flits_valid && route_w == 3'd0;
  logic want_local_n;
  assign want_local_n = __in_local_flits_valid && route_local == 3'd1;
  logic want_n_n;
  assign want_n_n = __in_n_flits_valid && route_n == 3'd1;
  logic want_s_n;
  assign want_s_n = __in_s_flits_valid && route_s == 3'd1;
  logic want_e_n;
  assign want_e_n = __in_e_flits_valid && route_e == 3'd1;
  logic want_w_n;
  assign want_w_n = __in_w_flits_valid && route_w == 3'd1;
  logic want_local_s;
  assign want_local_s = __in_local_flits_valid && route_local == 3'd2;
  logic want_n_s;
  assign want_n_s = __in_n_flits_valid && route_n == 3'd2;
  logic want_s_s;
  assign want_s_s = __in_s_flits_valid && route_s == 3'd2;
  logic want_e_s;
  assign want_e_s = __in_e_flits_valid && route_e == 3'd2;
  logic want_w_s;
  assign want_w_s = __in_w_flits_valid && route_w == 3'd2;
  logic want_local_e;
  assign want_local_e = __in_local_flits_valid && route_local == 3'd3;
  logic want_n_e;
  assign want_n_e = __in_n_flits_valid && route_n == 3'd3;
  logic want_s_e;
  assign want_s_e = __in_s_flits_valid && route_s == 3'd3;
  logic want_e_e;
  assign want_e_e = __in_e_flits_valid && route_e == 3'd3;
  logic want_w_e;
  assign want_w_e = __in_w_flits_valid && route_w == 3'd3;
  logic want_local_w;
  assign want_local_w = __in_local_flits_valid && route_local == 3'd4;
  logic want_n_w;
  assign want_n_w = __in_n_flits_valid && route_n == 3'd4;
  logic want_s_w;
  assign want_s_w = __in_s_flits_valid && route_s == 3'd4;
  logic want_e_w;
  assign want_e_w = __in_e_flits_valid && route_e == 3'd4;
  logic want_w_w;
  assign want_w_w = __in_w_flits_valid && route_w == 3'd4;
  logic [2:0] pick_local;
  assign pick_local = __out_local_flits_can_send && want_local_local ? 3'd1 : __out_local_flits_can_send && want_n_local ? 3'd2 : __out_local_flits_can_send && want_s_local ? 3'd3 : __out_local_flits_can_send && want_e_local ? 3'd4 : __out_local_flits_can_send && want_w_local ? 3'd5 : 3'd0;
  logic [2:0] pick_n;
  assign pick_n = __out_n_flits_can_send && want_local_n ? 3'd1 : __out_n_flits_can_send && want_n_n ? 3'd2 : __out_n_flits_can_send && want_s_n ? 3'd3 : __out_n_flits_can_send && want_e_n ? 3'd4 : __out_n_flits_can_send && want_w_n ? 3'd5 : 3'd0;
  logic [2:0] pick_s;
  assign pick_s = __out_s_flits_can_send && want_local_s ? 3'd1 : __out_s_flits_can_send && want_n_s ? 3'd2 : __out_s_flits_can_send && want_s_s ? 3'd3 : __out_s_flits_can_send && want_e_s ? 3'd4 : __out_s_flits_can_send && want_w_s ? 3'd5 : 3'd0;
  logic [2:0] pick_e;
  assign pick_e = __out_e_flits_can_send && want_local_e ? 3'd1 : __out_e_flits_can_send && want_n_e ? 3'd2 : __out_e_flits_can_send && want_s_e ? 3'd3 : __out_e_flits_can_send && want_e_e ? 3'd4 : __out_e_flits_can_send && want_w_e ? 3'd5 : 3'd0;
  logic [2:0] pick_w;
  assign pick_w = __out_w_flits_can_send && want_local_w ? 3'd1 : __out_w_flits_can_send && want_n_w ? 3'd2 : __out_w_flits_can_send && want_s_w ? 3'd3 : __out_w_flits_can_send && want_e_w ? 3'd4 : __out_w_flits_can_send && want_w_w ? 3'd5 : 3'd0;
  logic served_local;
  assign served_local = pick_local == 3'd1 || pick_n == 3'd1 || pick_s == 3'd1 || pick_e == 3'd1 || pick_w == 3'd1;
  logic served_n;
  assign served_n = pick_local == 3'd2 || pick_n == 3'd2 || pick_s == 3'd2 || pick_e == 3'd2 || pick_w == 3'd2;
  logic served_s;
  assign served_s = pick_local == 3'd3 || pick_n == 3'd3 || pick_s == 3'd3 || pick_e == 3'd3 || pick_w == 3'd3;
  logic served_e;
  assign served_e = pick_local == 3'd4 || pick_n == 3'd4 || pick_s == 3'd4 || pick_e == 3'd4 || pick_w == 3'd4;
  logic served_w;
  assign served_w = pick_local == 3'd5 || pick_n == 3'd5 || pick_s == 3'd5 || pick_e == 3'd5 || pick_w == 3'd5;
  assign out_local_flits_send_valid = pick_local != 3'd0;
  assign out_local_flits_send_data = pick_local == 3'd1 ? lx_data : pick_local == 3'd2 ? nx_data : pick_local == 3'd3 ? sx_data : pick_local == 3'd4 ? ex_data : wx_data;
  assign out_n_flits_send_valid = pick_n != 3'd0;
  assign out_n_flits_send_data = pick_n == 3'd1 ? lx_data : pick_n == 3'd2 ? nx_data : pick_n == 3'd3 ? sx_data : pick_n == 3'd4 ? ex_data : wx_data;
  assign out_s_flits_send_valid = pick_s != 3'd0;
  assign out_s_flits_send_data = pick_s == 3'd1 ? lx_data : pick_s == 3'd2 ? nx_data : pick_s == 3'd3 ? sx_data : pick_s == 3'd4 ? ex_data : wx_data;
  assign out_e_flits_send_valid = pick_e != 3'd0;
  assign out_e_flits_send_data = pick_e == 3'd1 ? lx_data : pick_e == 3'd2 ? nx_data : pick_e == 3'd3 ? sx_data : pick_e == 3'd4 ? ex_data : wx_data;
  assign out_w_flits_send_valid = pick_w != 3'd0;
  assign out_w_flits_send_data = pick_w == 3'd1 ? lx_data : pick_w == 3'd2 ? nx_data : pick_w == 3'd3 ? sx_data : pick_w == 3'd4 ? ex_data : wx_data;
  assign in_local_flits_credit_return = served_local;
  assign in_n_flits_credit_return = served_n;
  assign in_s_flits_credit_return = served_s;
  assign in_e_flits_credit_return = served_e;
  assign in_w_flits_credit_return = served_w;
  
  // Auto-generated credit_channel state (PR #3b-ii, sender side)
  logic [$clog2((4) + 1) - 1:0] __out_local_flits_credit;
  wire  __out_local_flits_can_send = __out_local_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_local_flits_credit <= 4;
    end else begin
      if (out_local_flits_send_valid && !out_local_flits_credit_return) __out_local_flits_credit <= __out_local_flits_credit - 1;
      else if (out_local_flits_credit_return && !out_local_flits_send_valid) __out_local_flits_credit <= __out_local_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_n_flits_credit;
  wire  __out_n_flits_can_send = __out_n_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_n_flits_credit <= 4;
    end else begin
      if (out_n_flits_send_valid && !out_n_flits_credit_return) __out_n_flits_credit <= __out_n_flits_credit - 1;
      else if (out_n_flits_credit_return && !out_n_flits_send_valid) __out_n_flits_credit <= __out_n_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_s_flits_credit;
  wire  __out_s_flits_can_send = __out_s_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_s_flits_credit <= 4;
    end else begin
      if (out_s_flits_send_valid && !out_s_flits_credit_return) __out_s_flits_credit <= __out_s_flits_credit - 1;
      else if (out_s_flits_credit_return && !out_s_flits_send_valid) __out_s_flits_credit <= __out_s_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_e_flits_credit;
  wire  __out_e_flits_can_send = __out_e_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_e_flits_credit <= 4;
    end else begin
      if (out_e_flits_send_valid && !out_e_flits_credit_return) __out_e_flits_credit <= __out_e_flits_credit - 1;
      else if (out_e_flits_credit_return && !out_e_flits_send_valid) __out_e_flits_credit <= __out_e_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_w_flits_credit;
  wire  __out_w_flits_can_send = __out_w_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_w_flits_credit <= 4;
    end else begin
      if (out_w_flits_send_valid && !out_w_flits_credit_return) __out_w_flits_credit <= __out_w_flits_credit - 1;
      else if (out_w_flits_credit_return && !out_w_flits_send_valid) __out_w_flits_credit <= __out_w_flits_credit + 1;
    end
  end
  
  // Auto-generated credit_channel target-side FIFO (PR #3b-iii)
  logic [(32) - 1:0] __in_local_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_local_flits_occ;
  wire  __in_local_flits_valid = __in_local_flits_occ != 0;
  wire [(32) - 1:0] __in_local_flits_data = __in_local_flits_buf[__in_local_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_local_flits_head <= 0;
      __in_local_flits_tail <= 0;
      __in_local_flits_occ  <= 0;
    end else begin
      if (in_local_flits_send_valid) begin
        __in_local_flits_buf[__in_local_flits_tail] <= in_local_flits_send_data;
        __in_local_flits_tail <= (__in_local_flits_tail + 1) % (4);
      end
      if ((in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_head <= (__in_local_flits_head + 1) % (4);
      if (in_local_flits_send_valid && !(in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ + 1;
      else if (!in_local_flits_send_valid &&  (in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_n_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_n_flits_occ;
  wire  __in_n_flits_valid = __in_n_flits_occ != 0;
  wire [(32) - 1:0] __in_n_flits_data = __in_n_flits_buf[__in_n_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_n_flits_head <= 0;
      __in_n_flits_tail <= 0;
      __in_n_flits_occ  <= 0;
    end else begin
      if (in_n_flits_send_valid) begin
        __in_n_flits_buf[__in_n_flits_tail] <= in_n_flits_send_data;
        __in_n_flits_tail <= (__in_n_flits_tail + 1) % (4);
      end
      if ((in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_head <= (__in_n_flits_head + 1) % (4);
      if (in_n_flits_send_valid && !(in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ + 1;
      else if (!in_n_flits_send_valid &&  (in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_s_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_s_flits_occ;
  wire  __in_s_flits_valid = __in_s_flits_occ != 0;
  wire [(32) - 1:0] __in_s_flits_data = __in_s_flits_buf[__in_s_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_s_flits_head <= 0;
      __in_s_flits_tail <= 0;
      __in_s_flits_occ  <= 0;
    end else begin
      if (in_s_flits_send_valid) begin
        __in_s_flits_buf[__in_s_flits_tail] <= in_s_flits_send_data;
        __in_s_flits_tail <= (__in_s_flits_tail + 1) % (4);
      end
      if ((in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_head <= (__in_s_flits_head + 1) % (4);
      if (in_s_flits_send_valid && !(in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ + 1;
      else if (!in_s_flits_send_valid &&  (in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_e_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_e_flits_occ;
  wire  __in_e_flits_valid = __in_e_flits_occ != 0;
  wire [(32) - 1:0] __in_e_flits_data = __in_e_flits_buf[__in_e_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_e_flits_head <= 0;
      __in_e_flits_tail <= 0;
      __in_e_flits_occ  <= 0;
    end else begin
      if (in_e_flits_send_valid) begin
        __in_e_flits_buf[__in_e_flits_tail] <= in_e_flits_send_data;
        __in_e_flits_tail <= (__in_e_flits_tail + 1) % (4);
      end
      if ((in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_head <= (__in_e_flits_head + 1) % (4);
      if (in_e_flits_send_valid && !(in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ + 1;
      else if (!in_e_flits_send_valid &&  (in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_w_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_w_flits_occ;
  wire  __in_w_flits_valid = __in_w_flits_occ != 0;
  wire [(32) - 1:0] __in_w_flits_data = __in_w_flits_buf[__in_w_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_w_flits_head <= 0;
      __in_w_flits_tail <= 0;
      __in_w_flits_occ  <= 0;
    end else begin
      if (in_w_flits_send_valid) begin
        __in_w_flits_buf[__in_w_flits_tail] <= in_w_flits_send_data;
        __in_w_flits_tail <= (__in_w_flits_tail + 1) % (4);
      end
      if ((in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_head <= (__in_w_flits_head + 1) % (4);
      if (in_w_flits_send_valid && !(in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ + 1;
      else if (!in_w_flits_send_valid &&  (in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ - 1;
    end
  end
  
  // synopsys translate_off
  // Auto-generated credit_channel protocol assertions (Tier 2)
  _auto_cc_out_local_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_local_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_0_Y_3._auto_cc_out_local_flits_credit_bounds");
  _auto_cc_out_local_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_local_flits_send_valid |-> __out_local_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_0_Y_3._auto_cc_out_local_flits_send_requires_credit");
  _auto_cc_out_n_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_n_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_0_Y_3._auto_cc_out_n_flits_credit_bounds");
  _auto_cc_out_n_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_n_flits_send_valid |-> __out_n_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_0_Y_3._auto_cc_out_n_flits_send_requires_credit");
  _auto_cc_out_s_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_s_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_0_Y_3._auto_cc_out_s_flits_credit_bounds");
  _auto_cc_out_s_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_s_flits_send_valid |-> __out_s_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_0_Y_3._auto_cc_out_s_flits_send_requires_credit");
  _auto_cc_out_e_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_e_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_0_Y_3._auto_cc_out_e_flits_credit_bounds");
  _auto_cc_out_e_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_e_flits_send_valid |-> __out_e_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_0_Y_3._auto_cc_out_e_flits_send_requires_credit");
  _auto_cc_out_w_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_w_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_0_Y_3._auto_cc_out_w_flits_credit_bounds");
  _auto_cc_out_w_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_w_flits_send_valid |-> __out_w_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_0_Y_3._auto_cc_out_w_flits_send_requires_credit");
  _auto_cc_in_local_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_local_flits_credit_return |-> __in_local_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_0_Y_3._auto_cc_in_local_flits_credit_return_requires_buffered");
  _auto_cc_in_n_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_n_flits_credit_return |-> __in_n_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_0_Y_3._auto_cc_in_n_flits_credit_return_requires_buffered");
  _auto_cc_in_s_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_s_flits_credit_return |-> __in_s_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_0_Y_3._auto_cc_in_s_flits_credit_return_requires_buffered");
  _auto_cc_in_e_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_e_flits_credit_return |-> __in_e_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_0_Y_3._auto_cc_in_e_flits_credit_return_requires_buffered");
  _auto_cc_in_w_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_w_flits_credit_return |-> __in_w_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_0_Y_3._auto_cc_in_w_flits_credit_return_requires_buffered");
  // synopsys translate_on

endmodule

module Router__X_1_Y_3 #(
  parameter int X = 1,
  parameter int Y = 3
) (
  input logic clk,
  input logic rst,
  input logic in_local_flits_send_valid,
  input logic [31:0] in_local_flits_send_data,
  output logic in_local_flits_credit_return,
  input logic in_n_flits_send_valid,
  input logic [31:0] in_n_flits_send_data,
  output logic in_n_flits_credit_return,
  input logic in_s_flits_send_valid,
  input logic [31:0] in_s_flits_send_data,
  output logic in_s_flits_credit_return,
  input logic in_e_flits_send_valid,
  input logic [31:0] in_e_flits_send_data,
  output logic in_e_flits_credit_return,
  input logic in_w_flits_send_valid,
  input logic [31:0] in_w_flits_send_data,
  output logic in_w_flits_credit_return,
  output logic out_local_flits_send_valid,
  output logic [31:0] out_local_flits_send_data,
  input logic out_local_flits_credit_return,
  output logic out_n_flits_send_valid,
  output logic [31:0] out_n_flits_send_data,
  input logic out_n_flits_credit_return,
  output logic out_s_flits_send_valid,
  output logic [31:0] out_s_flits_send_data,
  input logic out_s_flits_credit_return,
  output logic out_e_flits_send_valid,
  output logic [31:0] out_e_flits_send_data,
  input logic out_e_flits_credit_return,
  output logic out_w_flits_send_valid,
  output logic [31:0] out_w_flits_send_data,
  input logic out_w_flits_credit_return
);

  logic [31:0] lx_data;
  assign lx_data = __in_local_flits_data;
  logic [31:0] nx_data;
  assign nx_data = __in_n_flits_data;
  logic [31:0] sx_data;
  assign sx_data = __in_s_flits_data;
  logic [31:0] ex_data;
  assign ex_data = __in_e_flits_data;
  logic [31:0] wx_data;
  assign wx_data = __in_w_flits_data;
  logic [1:0] lx_dx;
  assign lx_dx = lx_data[1:0];
  logic [1:0] lx_dy;
  assign lx_dy = lx_data[3:2];
  logic [1:0] nx_dx;
  assign nx_dx = nx_data[1:0];
  logic [1:0] nx_dy;
  assign nx_dy = nx_data[3:2];
  logic [1:0] sx_dx;
  assign sx_dx = sx_data[1:0];
  logic [1:0] sx_dy;
  assign sx_dy = sx_data[3:2];
  logic [1:0] ex_dx;
  assign ex_dx = ex_data[1:0];
  logic [1:0] ex_dy;
  assign ex_dy = ex_data[3:2];
  logic [1:0] wx_dx;
  assign wx_dx = wx_data[1:0];
  logic [1:0] wx_dy;
  assign wx_dy = wx_data[3:2];
  logic [1:0] mx;
  assign mx = X;
  logic [1:0] my;
  assign my = Y;
  logic [2:0] route_local;
  assign route_local = lx_dx > mx ? 3'd3 : lx_dx < mx ? 3'd4 : lx_dy > my ? 3'd1 : lx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_n;
  assign route_n = nx_dx > mx ? 3'd3 : nx_dx < mx ? 3'd4 : nx_dy > my ? 3'd1 : nx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_s;
  assign route_s = sx_dx > mx ? 3'd3 : sx_dx < mx ? 3'd4 : sx_dy > my ? 3'd1 : sx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_e;
  assign route_e = ex_dx > mx ? 3'd3 : ex_dx < mx ? 3'd4 : ex_dy > my ? 3'd1 : ex_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_w;
  assign route_w = wx_dx > mx ? 3'd3 : wx_dx < mx ? 3'd4 : wx_dy > my ? 3'd1 : wx_dy < my ? 3'd2 : 3'd0;
  logic want_local_local;
  assign want_local_local = __in_local_flits_valid && route_local == 3'd0;
  logic want_n_local;
  assign want_n_local = __in_n_flits_valid && route_n == 3'd0;
  logic want_s_local;
  assign want_s_local = __in_s_flits_valid && route_s == 3'd0;
  logic want_e_local;
  assign want_e_local = __in_e_flits_valid && route_e == 3'd0;
  logic want_w_local;
  assign want_w_local = __in_w_flits_valid && route_w == 3'd0;
  logic want_local_n;
  assign want_local_n = __in_local_flits_valid && route_local == 3'd1;
  logic want_n_n;
  assign want_n_n = __in_n_flits_valid && route_n == 3'd1;
  logic want_s_n;
  assign want_s_n = __in_s_flits_valid && route_s == 3'd1;
  logic want_e_n;
  assign want_e_n = __in_e_flits_valid && route_e == 3'd1;
  logic want_w_n;
  assign want_w_n = __in_w_flits_valid && route_w == 3'd1;
  logic want_local_s;
  assign want_local_s = __in_local_flits_valid && route_local == 3'd2;
  logic want_n_s;
  assign want_n_s = __in_n_flits_valid && route_n == 3'd2;
  logic want_s_s;
  assign want_s_s = __in_s_flits_valid && route_s == 3'd2;
  logic want_e_s;
  assign want_e_s = __in_e_flits_valid && route_e == 3'd2;
  logic want_w_s;
  assign want_w_s = __in_w_flits_valid && route_w == 3'd2;
  logic want_local_e;
  assign want_local_e = __in_local_flits_valid && route_local == 3'd3;
  logic want_n_e;
  assign want_n_e = __in_n_flits_valid && route_n == 3'd3;
  logic want_s_e;
  assign want_s_e = __in_s_flits_valid && route_s == 3'd3;
  logic want_e_e;
  assign want_e_e = __in_e_flits_valid && route_e == 3'd3;
  logic want_w_e;
  assign want_w_e = __in_w_flits_valid && route_w == 3'd3;
  logic want_local_w;
  assign want_local_w = __in_local_flits_valid && route_local == 3'd4;
  logic want_n_w;
  assign want_n_w = __in_n_flits_valid && route_n == 3'd4;
  logic want_s_w;
  assign want_s_w = __in_s_flits_valid && route_s == 3'd4;
  logic want_e_w;
  assign want_e_w = __in_e_flits_valid && route_e == 3'd4;
  logic want_w_w;
  assign want_w_w = __in_w_flits_valid && route_w == 3'd4;
  logic [2:0] pick_local;
  assign pick_local = __out_local_flits_can_send && want_local_local ? 3'd1 : __out_local_flits_can_send && want_n_local ? 3'd2 : __out_local_flits_can_send && want_s_local ? 3'd3 : __out_local_flits_can_send && want_e_local ? 3'd4 : __out_local_flits_can_send && want_w_local ? 3'd5 : 3'd0;
  logic [2:0] pick_n;
  assign pick_n = __out_n_flits_can_send && want_local_n ? 3'd1 : __out_n_flits_can_send && want_n_n ? 3'd2 : __out_n_flits_can_send && want_s_n ? 3'd3 : __out_n_flits_can_send && want_e_n ? 3'd4 : __out_n_flits_can_send && want_w_n ? 3'd5 : 3'd0;
  logic [2:0] pick_s;
  assign pick_s = __out_s_flits_can_send && want_local_s ? 3'd1 : __out_s_flits_can_send && want_n_s ? 3'd2 : __out_s_flits_can_send && want_s_s ? 3'd3 : __out_s_flits_can_send && want_e_s ? 3'd4 : __out_s_flits_can_send && want_w_s ? 3'd5 : 3'd0;
  logic [2:0] pick_e;
  assign pick_e = __out_e_flits_can_send && want_local_e ? 3'd1 : __out_e_flits_can_send && want_n_e ? 3'd2 : __out_e_flits_can_send && want_s_e ? 3'd3 : __out_e_flits_can_send && want_e_e ? 3'd4 : __out_e_flits_can_send && want_w_e ? 3'd5 : 3'd0;
  logic [2:0] pick_w;
  assign pick_w = __out_w_flits_can_send && want_local_w ? 3'd1 : __out_w_flits_can_send && want_n_w ? 3'd2 : __out_w_flits_can_send && want_s_w ? 3'd3 : __out_w_flits_can_send && want_e_w ? 3'd4 : __out_w_flits_can_send && want_w_w ? 3'd5 : 3'd0;
  logic served_local;
  assign served_local = pick_local == 3'd1 || pick_n == 3'd1 || pick_s == 3'd1 || pick_e == 3'd1 || pick_w == 3'd1;
  logic served_n;
  assign served_n = pick_local == 3'd2 || pick_n == 3'd2 || pick_s == 3'd2 || pick_e == 3'd2 || pick_w == 3'd2;
  logic served_s;
  assign served_s = pick_local == 3'd3 || pick_n == 3'd3 || pick_s == 3'd3 || pick_e == 3'd3 || pick_w == 3'd3;
  logic served_e;
  assign served_e = pick_local == 3'd4 || pick_n == 3'd4 || pick_s == 3'd4 || pick_e == 3'd4 || pick_w == 3'd4;
  logic served_w;
  assign served_w = pick_local == 3'd5 || pick_n == 3'd5 || pick_s == 3'd5 || pick_e == 3'd5 || pick_w == 3'd5;
  assign out_local_flits_send_valid = pick_local != 3'd0;
  assign out_local_flits_send_data = pick_local == 3'd1 ? lx_data : pick_local == 3'd2 ? nx_data : pick_local == 3'd3 ? sx_data : pick_local == 3'd4 ? ex_data : wx_data;
  assign out_n_flits_send_valid = pick_n != 3'd0;
  assign out_n_flits_send_data = pick_n == 3'd1 ? lx_data : pick_n == 3'd2 ? nx_data : pick_n == 3'd3 ? sx_data : pick_n == 3'd4 ? ex_data : wx_data;
  assign out_s_flits_send_valid = pick_s != 3'd0;
  assign out_s_flits_send_data = pick_s == 3'd1 ? lx_data : pick_s == 3'd2 ? nx_data : pick_s == 3'd3 ? sx_data : pick_s == 3'd4 ? ex_data : wx_data;
  assign out_e_flits_send_valid = pick_e != 3'd0;
  assign out_e_flits_send_data = pick_e == 3'd1 ? lx_data : pick_e == 3'd2 ? nx_data : pick_e == 3'd3 ? sx_data : pick_e == 3'd4 ? ex_data : wx_data;
  assign out_w_flits_send_valid = pick_w != 3'd0;
  assign out_w_flits_send_data = pick_w == 3'd1 ? lx_data : pick_w == 3'd2 ? nx_data : pick_w == 3'd3 ? sx_data : pick_w == 3'd4 ? ex_data : wx_data;
  assign in_local_flits_credit_return = served_local;
  assign in_n_flits_credit_return = served_n;
  assign in_s_flits_credit_return = served_s;
  assign in_e_flits_credit_return = served_e;
  assign in_w_flits_credit_return = served_w;
  
  // Auto-generated credit_channel state (PR #3b-ii, sender side)
  logic [$clog2((4) + 1) - 1:0] __out_local_flits_credit;
  wire  __out_local_flits_can_send = __out_local_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_local_flits_credit <= 4;
    end else begin
      if (out_local_flits_send_valid && !out_local_flits_credit_return) __out_local_flits_credit <= __out_local_flits_credit - 1;
      else if (out_local_flits_credit_return && !out_local_flits_send_valid) __out_local_flits_credit <= __out_local_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_n_flits_credit;
  wire  __out_n_flits_can_send = __out_n_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_n_flits_credit <= 4;
    end else begin
      if (out_n_flits_send_valid && !out_n_flits_credit_return) __out_n_flits_credit <= __out_n_flits_credit - 1;
      else if (out_n_flits_credit_return && !out_n_flits_send_valid) __out_n_flits_credit <= __out_n_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_s_flits_credit;
  wire  __out_s_flits_can_send = __out_s_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_s_flits_credit <= 4;
    end else begin
      if (out_s_flits_send_valid && !out_s_flits_credit_return) __out_s_flits_credit <= __out_s_flits_credit - 1;
      else if (out_s_flits_credit_return && !out_s_flits_send_valid) __out_s_flits_credit <= __out_s_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_e_flits_credit;
  wire  __out_e_flits_can_send = __out_e_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_e_flits_credit <= 4;
    end else begin
      if (out_e_flits_send_valid && !out_e_flits_credit_return) __out_e_flits_credit <= __out_e_flits_credit - 1;
      else if (out_e_flits_credit_return && !out_e_flits_send_valid) __out_e_flits_credit <= __out_e_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_w_flits_credit;
  wire  __out_w_flits_can_send = __out_w_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_w_flits_credit <= 4;
    end else begin
      if (out_w_flits_send_valid && !out_w_flits_credit_return) __out_w_flits_credit <= __out_w_flits_credit - 1;
      else if (out_w_flits_credit_return && !out_w_flits_send_valid) __out_w_flits_credit <= __out_w_flits_credit + 1;
    end
  end
  
  // Auto-generated credit_channel target-side FIFO (PR #3b-iii)
  logic [(32) - 1:0] __in_local_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_local_flits_occ;
  wire  __in_local_flits_valid = __in_local_flits_occ != 0;
  wire [(32) - 1:0] __in_local_flits_data = __in_local_flits_buf[__in_local_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_local_flits_head <= 0;
      __in_local_flits_tail <= 0;
      __in_local_flits_occ  <= 0;
    end else begin
      if (in_local_flits_send_valid) begin
        __in_local_flits_buf[__in_local_flits_tail] <= in_local_flits_send_data;
        __in_local_flits_tail <= (__in_local_flits_tail + 1) % (4);
      end
      if ((in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_head <= (__in_local_flits_head + 1) % (4);
      if (in_local_flits_send_valid && !(in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ + 1;
      else if (!in_local_flits_send_valid &&  (in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_n_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_n_flits_occ;
  wire  __in_n_flits_valid = __in_n_flits_occ != 0;
  wire [(32) - 1:0] __in_n_flits_data = __in_n_flits_buf[__in_n_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_n_flits_head <= 0;
      __in_n_flits_tail <= 0;
      __in_n_flits_occ  <= 0;
    end else begin
      if (in_n_flits_send_valid) begin
        __in_n_flits_buf[__in_n_flits_tail] <= in_n_flits_send_data;
        __in_n_flits_tail <= (__in_n_flits_tail + 1) % (4);
      end
      if ((in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_head <= (__in_n_flits_head + 1) % (4);
      if (in_n_flits_send_valid && !(in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ + 1;
      else if (!in_n_flits_send_valid &&  (in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_s_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_s_flits_occ;
  wire  __in_s_flits_valid = __in_s_flits_occ != 0;
  wire [(32) - 1:0] __in_s_flits_data = __in_s_flits_buf[__in_s_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_s_flits_head <= 0;
      __in_s_flits_tail <= 0;
      __in_s_flits_occ  <= 0;
    end else begin
      if (in_s_flits_send_valid) begin
        __in_s_flits_buf[__in_s_flits_tail] <= in_s_flits_send_data;
        __in_s_flits_tail <= (__in_s_flits_tail + 1) % (4);
      end
      if ((in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_head <= (__in_s_flits_head + 1) % (4);
      if (in_s_flits_send_valid && !(in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ + 1;
      else if (!in_s_flits_send_valid &&  (in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_e_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_e_flits_occ;
  wire  __in_e_flits_valid = __in_e_flits_occ != 0;
  wire [(32) - 1:0] __in_e_flits_data = __in_e_flits_buf[__in_e_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_e_flits_head <= 0;
      __in_e_flits_tail <= 0;
      __in_e_flits_occ  <= 0;
    end else begin
      if (in_e_flits_send_valid) begin
        __in_e_flits_buf[__in_e_flits_tail] <= in_e_flits_send_data;
        __in_e_flits_tail <= (__in_e_flits_tail + 1) % (4);
      end
      if ((in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_head <= (__in_e_flits_head + 1) % (4);
      if (in_e_flits_send_valid && !(in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ + 1;
      else if (!in_e_flits_send_valid &&  (in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_w_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_w_flits_occ;
  wire  __in_w_flits_valid = __in_w_flits_occ != 0;
  wire [(32) - 1:0] __in_w_flits_data = __in_w_flits_buf[__in_w_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_w_flits_head <= 0;
      __in_w_flits_tail <= 0;
      __in_w_flits_occ  <= 0;
    end else begin
      if (in_w_flits_send_valid) begin
        __in_w_flits_buf[__in_w_flits_tail] <= in_w_flits_send_data;
        __in_w_flits_tail <= (__in_w_flits_tail + 1) % (4);
      end
      if ((in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_head <= (__in_w_flits_head + 1) % (4);
      if (in_w_flits_send_valid && !(in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ + 1;
      else if (!in_w_flits_send_valid &&  (in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ - 1;
    end
  end
  
  // synopsys translate_off
  // Auto-generated credit_channel protocol assertions (Tier 2)
  _auto_cc_out_local_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_local_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_1_Y_3._auto_cc_out_local_flits_credit_bounds");
  _auto_cc_out_local_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_local_flits_send_valid |-> __out_local_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_1_Y_3._auto_cc_out_local_flits_send_requires_credit");
  _auto_cc_out_n_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_n_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_1_Y_3._auto_cc_out_n_flits_credit_bounds");
  _auto_cc_out_n_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_n_flits_send_valid |-> __out_n_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_1_Y_3._auto_cc_out_n_flits_send_requires_credit");
  _auto_cc_out_s_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_s_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_1_Y_3._auto_cc_out_s_flits_credit_bounds");
  _auto_cc_out_s_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_s_flits_send_valid |-> __out_s_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_1_Y_3._auto_cc_out_s_flits_send_requires_credit");
  _auto_cc_out_e_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_e_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_1_Y_3._auto_cc_out_e_flits_credit_bounds");
  _auto_cc_out_e_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_e_flits_send_valid |-> __out_e_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_1_Y_3._auto_cc_out_e_flits_send_requires_credit");
  _auto_cc_out_w_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_w_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_1_Y_3._auto_cc_out_w_flits_credit_bounds");
  _auto_cc_out_w_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_w_flits_send_valid |-> __out_w_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_1_Y_3._auto_cc_out_w_flits_send_requires_credit");
  _auto_cc_in_local_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_local_flits_credit_return |-> __in_local_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_1_Y_3._auto_cc_in_local_flits_credit_return_requires_buffered");
  _auto_cc_in_n_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_n_flits_credit_return |-> __in_n_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_1_Y_3._auto_cc_in_n_flits_credit_return_requires_buffered");
  _auto_cc_in_s_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_s_flits_credit_return |-> __in_s_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_1_Y_3._auto_cc_in_s_flits_credit_return_requires_buffered");
  _auto_cc_in_e_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_e_flits_credit_return |-> __in_e_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_1_Y_3._auto_cc_in_e_flits_credit_return_requires_buffered");
  _auto_cc_in_w_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_w_flits_credit_return |-> __in_w_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_1_Y_3._auto_cc_in_w_flits_credit_return_requires_buffered");
  // synopsys translate_on

endmodule

module Router__X_2_Y_3 #(
  parameter int X = 2,
  parameter int Y = 3
) (
  input logic clk,
  input logic rst,
  input logic in_local_flits_send_valid,
  input logic [31:0] in_local_flits_send_data,
  output logic in_local_flits_credit_return,
  input logic in_n_flits_send_valid,
  input logic [31:0] in_n_flits_send_data,
  output logic in_n_flits_credit_return,
  input logic in_s_flits_send_valid,
  input logic [31:0] in_s_flits_send_data,
  output logic in_s_flits_credit_return,
  input logic in_e_flits_send_valid,
  input logic [31:0] in_e_flits_send_data,
  output logic in_e_flits_credit_return,
  input logic in_w_flits_send_valid,
  input logic [31:0] in_w_flits_send_data,
  output logic in_w_flits_credit_return,
  output logic out_local_flits_send_valid,
  output logic [31:0] out_local_flits_send_data,
  input logic out_local_flits_credit_return,
  output logic out_n_flits_send_valid,
  output logic [31:0] out_n_flits_send_data,
  input logic out_n_flits_credit_return,
  output logic out_s_flits_send_valid,
  output logic [31:0] out_s_flits_send_data,
  input logic out_s_flits_credit_return,
  output logic out_e_flits_send_valid,
  output logic [31:0] out_e_flits_send_data,
  input logic out_e_flits_credit_return,
  output logic out_w_flits_send_valid,
  output logic [31:0] out_w_flits_send_data,
  input logic out_w_flits_credit_return
);

  logic [31:0] lx_data;
  assign lx_data = __in_local_flits_data;
  logic [31:0] nx_data;
  assign nx_data = __in_n_flits_data;
  logic [31:0] sx_data;
  assign sx_data = __in_s_flits_data;
  logic [31:0] ex_data;
  assign ex_data = __in_e_flits_data;
  logic [31:0] wx_data;
  assign wx_data = __in_w_flits_data;
  logic [1:0] lx_dx;
  assign lx_dx = lx_data[1:0];
  logic [1:0] lx_dy;
  assign lx_dy = lx_data[3:2];
  logic [1:0] nx_dx;
  assign nx_dx = nx_data[1:0];
  logic [1:0] nx_dy;
  assign nx_dy = nx_data[3:2];
  logic [1:0] sx_dx;
  assign sx_dx = sx_data[1:0];
  logic [1:0] sx_dy;
  assign sx_dy = sx_data[3:2];
  logic [1:0] ex_dx;
  assign ex_dx = ex_data[1:0];
  logic [1:0] ex_dy;
  assign ex_dy = ex_data[3:2];
  logic [1:0] wx_dx;
  assign wx_dx = wx_data[1:0];
  logic [1:0] wx_dy;
  assign wx_dy = wx_data[3:2];
  logic [1:0] mx;
  assign mx = X;
  logic [1:0] my;
  assign my = Y;
  logic [2:0] route_local;
  assign route_local = lx_dx > mx ? 3'd3 : lx_dx < mx ? 3'd4 : lx_dy > my ? 3'd1 : lx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_n;
  assign route_n = nx_dx > mx ? 3'd3 : nx_dx < mx ? 3'd4 : nx_dy > my ? 3'd1 : nx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_s;
  assign route_s = sx_dx > mx ? 3'd3 : sx_dx < mx ? 3'd4 : sx_dy > my ? 3'd1 : sx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_e;
  assign route_e = ex_dx > mx ? 3'd3 : ex_dx < mx ? 3'd4 : ex_dy > my ? 3'd1 : ex_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_w;
  assign route_w = wx_dx > mx ? 3'd3 : wx_dx < mx ? 3'd4 : wx_dy > my ? 3'd1 : wx_dy < my ? 3'd2 : 3'd0;
  logic want_local_local;
  assign want_local_local = __in_local_flits_valid && route_local == 3'd0;
  logic want_n_local;
  assign want_n_local = __in_n_flits_valid && route_n == 3'd0;
  logic want_s_local;
  assign want_s_local = __in_s_flits_valid && route_s == 3'd0;
  logic want_e_local;
  assign want_e_local = __in_e_flits_valid && route_e == 3'd0;
  logic want_w_local;
  assign want_w_local = __in_w_flits_valid && route_w == 3'd0;
  logic want_local_n;
  assign want_local_n = __in_local_flits_valid && route_local == 3'd1;
  logic want_n_n;
  assign want_n_n = __in_n_flits_valid && route_n == 3'd1;
  logic want_s_n;
  assign want_s_n = __in_s_flits_valid && route_s == 3'd1;
  logic want_e_n;
  assign want_e_n = __in_e_flits_valid && route_e == 3'd1;
  logic want_w_n;
  assign want_w_n = __in_w_flits_valid && route_w == 3'd1;
  logic want_local_s;
  assign want_local_s = __in_local_flits_valid && route_local == 3'd2;
  logic want_n_s;
  assign want_n_s = __in_n_flits_valid && route_n == 3'd2;
  logic want_s_s;
  assign want_s_s = __in_s_flits_valid && route_s == 3'd2;
  logic want_e_s;
  assign want_e_s = __in_e_flits_valid && route_e == 3'd2;
  logic want_w_s;
  assign want_w_s = __in_w_flits_valid && route_w == 3'd2;
  logic want_local_e;
  assign want_local_e = __in_local_flits_valid && route_local == 3'd3;
  logic want_n_e;
  assign want_n_e = __in_n_flits_valid && route_n == 3'd3;
  logic want_s_e;
  assign want_s_e = __in_s_flits_valid && route_s == 3'd3;
  logic want_e_e;
  assign want_e_e = __in_e_flits_valid && route_e == 3'd3;
  logic want_w_e;
  assign want_w_e = __in_w_flits_valid && route_w == 3'd3;
  logic want_local_w;
  assign want_local_w = __in_local_flits_valid && route_local == 3'd4;
  logic want_n_w;
  assign want_n_w = __in_n_flits_valid && route_n == 3'd4;
  logic want_s_w;
  assign want_s_w = __in_s_flits_valid && route_s == 3'd4;
  logic want_e_w;
  assign want_e_w = __in_e_flits_valid && route_e == 3'd4;
  logic want_w_w;
  assign want_w_w = __in_w_flits_valid && route_w == 3'd4;
  logic [2:0] pick_local;
  assign pick_local = __out_local_flits_can_send && want_local_local ? 3'd1 : __out_local_flits_can_send && want_n_local ? 3'd2 : __out_local_flits_can_send && want_s_local ? 3'd3 : __out_local_flits_can_send && want_e_local ? 3'd4 : __out_local_flits_can_send && want_w_local ? 3'd5 : 3'd0;
  logic [2:0] pick_n;
  assign pick_n = __out_n_flits_can_send && want_local_n ? 3'd1 : __out_n_flits_can_send && want_n_n ? 3'd2 : __out_n_flits_can_send && want_s_n ? 3'd3 : __out_n_flits_can_send && want_e_n ? 3'd4 : __out_n_flits_can_send && want_w_n ? 3'd5 : 3'd0;
  logic [2:0] pick_s;
  assign pick_s = __out_s_flits_can_send && want_local_s ? 3'd1 : __out_s_flits_can_send && want_n_s ? 3'd2 : __out_s_flits_can_send && want_s_s ? 3'd3 : __out_s_flits_can_send && want_e_s ? 3'd4 : __out_s_flits_can_send && want_w_s ? 3'd5 : 3'd0;
  logic [2:0] pick_e;
  assign pick_e = __out_e_flits_can_send && want_local_e ? 3'd1 : __out_e_flits_can_send && want_n_e ? 3'd2 : __out_e_flits_can_send && want_s_e ? 3'd3 : __out_e_flits_can_send && want_e_e ? 3'd4 : __out_e_flits_can_send && want_w_e ? 3'd5 : 3'd0;
  logic [2:0] pick_w;
  assign pick_w = __out_w_flits_can_send && want_local_w ? 3'd1 : __out_w_flits_can_send && want_n_w ? 3'd2 : __out_w_flits_can_send && want_s_w ? 3'd3 : __out_w_flits_can_send && want_e_w ? 3'd4 : __out_w_flits_can_send && want_w_w ? 3'd5 : 3'd0;
  logic served_local;
  assign served_local = pick_local == 3'd1 || pick_n == 3'd1 || pick_s == 3'd1 || pick_e == 3'd1 || pick_w == 3'd1;
  logic served_n;
  assign served_n = pick_local == 3'd2 || pick_n == 3'd2 || pick_s == 3'd2 || pick_e == 3'd2 || pick_w == 3'd2;
  logic served_s;
  assign served_s = pick_local == 3'd3 || pick_n == 3'd3 || pick_s == 3'd3 || pick_e == 3'd3 || pick_w == 3'd3;
  logic served_e;
  assign served_e = pick_local == 3'd4 || pick_n == 3'd4 || pick_s == 3'd4 || pick_e == 3'd4 || pick_w == 3'd4;
  logic served_w;
  assign served_w = pick_local == 3'd5 || pick_n == 3'd5 || pick_s == 3'd5 || pick_e == 3'd5 || pick_w == 3'd5;
  assign out_local_flits_send_valid = pick_local != 3'd0;
  assign out_local_flits_send_data = pick_local == 3'd1 ? lx_data : pick_local == 3'd2 ? nx_data : pick_local == 3'd3 ? sx_data : pick_local == 3'd4 ? ex_data : wx_data;
  assign out_n_flits_send_valid = pick_n != 3'd0;
  assign out_n_flits_send_data = pick_n == 3'd1 ? lx_data : pick_n == 3'd2 ? nx_data : pick_n == 3'd3 ? sx_data : pick_n == 3'd4 ? ex_data : wx_data;
  assign out_s_flits_send_valid = pick_s != 3'd0;
  assign out_s_flits_send_data = pick_s == 3'd1 ? lx_data : pick_s == 3'd2 ? nx_data : pick_s == 3'd3 ? sx_data : pick_s == 3'd4 ? ex_data : wx_data;
  assign out_e_flits_send_valid = pick_e != 3'd0;
  assign out_e_flits_send_data = pick_e == 3'd1 ? lx_data : pick_e == 3'd2 ? nx_data : pick_e == 3'd3 ? sx_data : pick_e == 3'd4 ? ex_data : wx_data;
  assign out_w_flits_send_valid = pick_w != 3'd0;
  assign out_w_flits_send_data = pick_w == 3'd1 ? lx_data : pick_w == 3'd2 ? nx_data : pick_w == 3'd3 ? sx_data : pick_w == 3'd4 ? ex_data : wx_data;
  assign in_local_flits_credit_return = served_local;
  assign in_n_flits_credit_return = served_n;
  assign in_s_flits_credit_return = served_s;
  assign in_e_flits_credit_return = served_e;
  assign in_w_flits_credit_return = served_w;
  
  // Auto-generated credit_channel state (PR #3b-ii, sender side)
  logic [$clog2((4) + 1) - 1:0] __out_local_flits_credit;
  wire  __out_local_flits_can_send = __out_local_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_local_flits_credit <= 4;
    end else begin
      if (out_local_flits_send_valid && !out_local_flits_credit_return) __out_local_flits_credit <= __out_local_flits_credit - 1;
      else if (out_local_flits_credit_return && !out_local_flits_send_valid) __out_local_flits_credit <= __out_local_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_n_flits_credit;
  wire  __out_n_flits_can_send = __out_n_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_n_flits_credit <= 4;
    end else begin
      if (out_n_flits_send_valid && !out_n_flits_credit_return) __out_n_flits_credit <= __out_n_flits_credit - 1;
      else if (out_n_flits_credit_return && !out_n_flits_send_valid) __out_n_flits_credit <= __out_n_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_s_flits_credit;
  wire  __out_s_flits_can_send = __out_s_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_s_flits_credit <= 4;
    end else begin
      if (out_s_flits_send_valid && !out_s_flits_credit_return) __out_s_flits_credit <= __out_s_flits_credit - 1;
      else if (out_s_flits_credit_return && !out_s_flits_send_valid) __out_s_flits_credit <= __out_s_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_e_flits_credit;
  wire  __out_e_flits_can_send = __out_e_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_e_flits_credit <= 4;
    end else begin
      if (out_e_flits_send_valid && !out_e_flits_credit_return) __out_e_flits_credit <= __out_e_flits_credit - 1;
      else if (out_e_flits_credit_return && !out_e_flits_send_valid) __out_e_flits_credit <= __out_e_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_w_flits_credit;
  wire  __out_w_flits_can_send = __out_w_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_w_flits_credit <= 4;
    end else begin
      if (out_w_flits_send_valid && !out_w_flits_credit_return) __out_w_flits_credit <= __out_w_flits_credit - 1;
      else if (out_w_flits_credit_return && !out_w_flits_send_valid) __out_w_flits_credit <= __out_w_flits_credit + 1;
    end
  end
  
  // Auto-generated credit_channel target-side FIFO (PR #3b-iii)
  logic [(32) - 1:0] __in_local_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_local_flits_occ;
  wire  __in_local_flits_valid = __in_local_flits_occ != 0;
  wire [(32) - 1:0] __in_local_flits_data = __in_local_flits_buf[__in_local_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_local_flits_head <= 0;
      __in_local_flits_tail <= 0;
      __in_local_flits_occ  <= 0;
    end else begin
      if (in_local_flits_send_valid) begin
        __in_local_flits_buf[__in_local_flits_tail] <= in_local_flits_send_data;
        __in_local_flits_tail <= (__in_local_flits_tail + 1) % (4);
      end
      if ((in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_head <= (__in_local_flits_head + 1) % (4);
      if (in_local_flits_send_valid && !(in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ + 1;
      else if (!in_local_flits_send_valid &&  (in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_n_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_n_flits_occ;
  wire  __in_n_flits_valid = __in_n_flits_occ != 0;
  wire [(32) - 1:0] __in_n_flits_data = __in_n_flits_buf[__in_n_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_n_flits_head <= 0;
      __in_n_flits_tail <= 0;
      __in_n_flits_occ  <= 0;
    end else begin
      if (in_n_flits_send_valid) begin
        __in_n_flits_buf[__in_n_flits_tail] <= in_n_flits_send_data;
        __in_n_flits_tail <= (__in_n_flits_tail + 1) % (4);
      end
      if ((in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_head <= (__in_n_flits_head + 1) % (4);
      if (in_n_flits_send_valid && !(in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ + 1;
      else if (!in_n_flits_send_valid &&  (in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_s_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_s_flits_occ;
  wire  __in_s_flits_valid = __in_s_flits_occ != 0;
  wire [(32) - 1:0] __in_s_flits_data = __in_s_flits_buf[__in_s_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_s_flits_head <= 0;
      __in_s_flits_tail <= 0;
      __in_s_flits_occ  <= 0;
    end else begin
      if (in_s_flits_send_valid) begin
        __in_s_flits_buf[__in_s_flits_tail] <= in_s_flits_send_data;
        __in_s_flits_tail <= (__in_s_flits_tail + 1) % (4);
      end
      if ((in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_head <= (__in_s_flits_head + 1) % (4);
      if (in_s_flits_send_valid && !(in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ + 1;
      else if (!in_s_flits_send_valid &&  (in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_e_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_e_flits_occ;
  wire  __in_e_flits_valid = __in_e_flits_occ != 0;
  wire [(32) - 1:0] __in_e_flits_data = __in_e_flits_buf[__in_e_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_e_flits_head <= 0;
      __in_e_flits_tail <= 0;
      __in_e_flits_occ  <= 0;
    end else begin
      if (in_e_flits_send_valid) begin
        __in_e_flits_buf[__in_e_flits_tail] <= in_e_flits_send_data;
        __in_e_flits_tail <= (__in_e_flits_tail + 1) % (4);
      end
      if ((in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_head <= (__in_e_flits_head + 1) % (4);
      if (in_e_flits_send_valid && !(in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ + 1;
      else if (!in_e_flits_send_valid &&  (in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_w_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_w_flits_occ;
  wire  __in_w_flits_valid = __in_w_flits_occ != 0;
  wire [(32) - 1:0] __in_w_flits_data = __in_w_flits_buf[__in_w_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_w_flits_head <= 0;
      __in_w_flits_tail <= 0;
      __in_w_flits_occ  <= 0;
    end else begin
      if (in_w_flits_send_valid) begin
        __in_w_flits_buf[__in_w_flits_tail] <= in_w_flits_send_data;
        __in_w_flits_tail <= (__in_w_flits_tail + 1) % (4);
      end
      if ((in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_head <= (__in_w_flits_head + 1) % (4);
      if (in_w_flits_send_valid && !(in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ + 1;
      else if (!in_w_flits_send_valid &&  (in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ - 1;
    end
  end
  
  // synopsys translate_off
  // Auto-generated credit_channel protocol assertions (Tier 2)
  _auto_cc_out_local_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_local_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_2_Y_3._auto_cc_out_local_flits_credit_bounds");
  _auto_cc_out_local_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_local_flits_send_valid |-> __out_local_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_2_Y_3._auto_cc_out_local_flits_send_requires_credit");
  _auto_cc_out_n_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_n_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_2_Y_3._auto_cc_out_n_flits_credit_bounds");
  _auto_cc_out_n_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_n_flits_send_valid |-> __out_n_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_2_Y_3._auto_cc_out_n_flits_send_requires_credit");
  _auto_cc_out_s_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_s_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_2_Y_3._auto_cc_out_s_flits_credit_bounds");
  _auto_cc_out_s_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_s_flits_send_valid |-> __out_s_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_2_Y_3._auto_cc_out_s_flits_send_requires_credit");
  _auto_cc_out_e_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_e_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_2_Y_3._auto_cc_out_e_flits_credit_bounds");
  _auto_cc_out_e_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_e_flits_send_valid |-> __out_e_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_2_Y_3._auto_cc_out_e_flits_send_requires_credit");
  _auto_cc_out_w_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_w_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_2_Y_3._auto_cc_out_w_flits_credit_bounds");
  _auto_cc_out_w_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_w_flits_send_valid |-> __out_w_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_2_Y_3._auto_cc_out_w_flits_send_requires_credit");
  _auto_cc_in_local_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_local_flits_credit_return |-> __in_local_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_2_Y_3._auto_cc_in_local_flits_credit_return_requires_buffered");
  _auto_cc_in_n_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_n_flits_credit_return |-> __in_n_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_2_Y_3._auto_cc_in_n_flits_credit_return_requires_buffered");
  _auto_cc_in_s_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_s_flits_credit_return |-> __in_s_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_2_Y_3._auto_cc_in_s_flits_credit_return_requires_buffered");
  _auto_cc_in_e_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_e_flits_credit_return |-> __in_e_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_2_Y_3._auto_cc_in_e_flits_credit_return_requires_buffered");
  _auto_cc_in_w_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_w_flits_credit_return |-> __in_w_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_2_Y_3._auto_cc_in_w_flits_credit_return_requires_buffered");
  // synopsys translate_on

endmodule

module Router__X_3_Y_3 #(
  parameter int X = 3,
  parameter int Y = 3
) (
  input logic clk,
  input logic rst,
  input logic in_local_flits_send_valid,
  input logic [31:0] in_local_flits_send_data,
  output logic in_local_flits_credit_return,
  input logic in_n_flits_send_valid,
  input logic [31:0] in_n_flits_send_data,
  output logic in_n_flits_credit_return,
  input logic in_s_flits_send_valid,
  input logic [31:0] in_s_flits_send_data,
  output logic in_s_flits_credit_return,
  input logic in_e_flits_send_valid,
  input logic [31:0] in_e_flits_send_data,
  output logic in_e_flits_credit_return,
  input logic in_w_flits_send_valid,
  input logic [31:0] in_w_flits_send_data,
  output logic in_w_flits_credit_return,
  output logic out_local_flits_send_valid,
  output logic [31:0] out_local_flits_send_data,
  input logic out_local_flits_credit_return,
  output logic out_n_flits_send_valid,
  output logic [31:0] out_n_flits_send_data,
  input logic out_n_flits_credit_return,
  output logic out_s_flits_send_valid,
  output logic [31:0] out_s_flits_send_data,
  input logic out_s_flits_credit_return,
  output logic out_e_flits_send_valid,
  output logic [31:0] out_e_flits_send_data,
  input logic out_e_flits_credit_return,
  output logic out_w_flits_send_valid,
  output logic [31:0] out_w_flits_send_data,
  input logic out_w_flits_credit_return
);

  logic [31:0] lx_data;
  assign lx_data = __in_local_flits_data;
  logic [31:0] nx_data;
  assign nx_data = __in_n_flits_data;
  logic [31:0] sx_data;
  assign sx_data = __in_s_flits_data;
  logic [31:0] ex_data;
  assign ex_data = __in_e_flits_data;
  logic [31:0] wx_data;
  assign wx_data = __in_w_flits_data;
  logic [1:0] lx_dx;
  assign lx_dx = lx_data[1:0];
  logic [1:0] lx_dy;
  assign lx_dy = lx_data[3:2];
  logic [1:0] nx_dx;
  assign nx_dx = nx_data[1:0];
  logic [1:0] nx_dy;
  assign nx_dy = nx_data[3:2];
  logic [1:0] sx_dx;
  assign sx_dx = sx_data[1:0];
  logic [1:0] sx_dy;
  assign sx_dy = sx_data[3:2];
  logic [1:0] ex_dx;
  assign ex_dx = ex_data[1:0];
  logic [1:0] ex_dy;
  assign ex_dy = ex_data[3:2];
  logic [1:0] wx_dx;
  assign wx_dx = wx_data[1:0];
  logic [1:0] wx_dy;
  assign wx_dy = wx_data[3:2];
  logic [1:0] mx;
  assign mx = X;
  logic [1:0] my;
  assign my = Y;
  logic [2:0] route_local;
  assign route_local = lx_dx > mx ? 3'd3 : lx_dx < mx ? 3'd4 : lx_dy > my ? 3'd1 : lx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_n;
  assign route_n = nx_dx > mx ? 3'd3 : nx_dx < mx ? 3'd4 : nx_dy > my ? 3'd1 : nx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_s;
  assign route_s = sx_dx > mx ? 3'd3 : sx_dx < mx ? 3'd4 : sx_dy > my ? 3'd1 : sx_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_e;
  assign route_e = ex_dx > mx ? 3'd3 : ex_dx < mx ? 3'd4 : ex_dy > my ? 3'd1 : ex_dy < my ? 3'd2 : 3'd0;
  logic [2:0] route_w;
  assign route_w = wx_dx > mx ? 3'd3 : wx_dx < mx ? 3'd4 : wx_dy > my ? 3'd1 : wx_dy < my ? 3'd2 : 3'd0;
  logic want_local_local;
  assign want_local_local = __in_local_flits_valid && route_local == 3'd0;
  logic want_n_local;
  assign want_n_local = __in_n_flits_valid && route_n == 3'd0;
  logic want_s_local;
  assign want_s_local = __in_s_flits_valid && route_s == 3'd0;
  logic want_e_local;
  assign want_e_local = __in_e_flits_valid && route_e == 3'd0;
  logic want_w_local;
  assign want_w_local = __in_w_flits_valid && route_w == 3'd0;
  logic want_local_n;
  assign want_local_n = __in_local_flits_valid && route_local == 3'd1;
  logic want_n_n;
  assign want_n_n = __in_n_flits_valid && route_n == 3'd1;
  logic want_s_n;
  assign want_s_n = __in_s_flits_valid && route_s == 3'd1;
  logic want_e_n;
  assign want_e_n = __in_e_flits_valid && route_e == 3'd1;
  logic want_w_n;
  assign want_w_n = __in_w_flits_valid && route_w == 3'd1;
  logic want_local_s;
  assign want_local_s = __in_local_flits_valid && route_local == 3'd2;
  logic want_n_s;
  assign want_n_s = __in_n_flits_valid && route_n == 3'd2;
  logic want_s_s;
  assign want_s_s = __in_s_flits_valid && route_s == 3'd2;
  logic want_e_s;
  assign want_e_s = __in_e_flits_valid && route_e == 3'd2;
  logic want_w_s;
  assign want_w_s = __in_w_flits_valid && route_w == 3'd2;
  logic want_local_e;
  assign want_local_e = __in_local_flits_valid && route_local == 3'd3;
  logic want_n_e;
  assign want_n_e = __in_n_flits_valid && route_n == 3'd3;
  logic want_s_e;
  assign want_s_e = __in_s_flits_valid && route_s == 3'd3;
  logic want_e_e;
  assign want_e_e = __in_e_flits_valid && route_e == 3'd3;
  logic want_w_e;
  assign want_w_e = __in_w_flits_valid && route_w == 3'd3;
  logic want_local_w;
  assign want_local_w = __in_local_flits_valid && route_local == 3'd4;
  logic want_n_w;
  assign want_n_w = __in_n_flits_valid && route_n == 3'd4;
  logic want_s_w;
  assign want_s_w = __in_s_flits_valid && route_s == 3'd4;
  logic want_e_w;
  assign want_e_w = __in_e_flits_valid && route_e == 3'd4;
  logic want_w_w;
  assign want_w_w = __in_w_flits_valid && route_w == 3'd4;
  logic [2:0] pick_local;
  assign pick_local = __out_local_flits_can_send && want_local_local ? 3'd1 : __out_local_flits_can_send && want_n_local ? 3'd2 : __out_local_flits_can_send && want_s_local ? 3'd3 : __out_local_flits_can_send && want_e_local ? 3'd4 : __out_local_flits_can_send && want_w_local ? 3'd5 : 3'd0;
  logic [2:0] pick_n;
  assign pick_n = __out_n_flits_can_send && want_local_n ? 3'd1 : __out_n_flits_can_send && want_n_n ? 3'd2 : __out_n_flits_can_send && want_s_n ? 3'd3 : __out_n_flits_can_send && want_e_n ? 3'd4 : __out_n_flits_can_send && want_w_n ? 3'd5 : 3'd0;
  logic [2:0] pick_s;
  assign pick_s = __out_s_flits_can_send && want_local_s ? 3'd1 : __out_s_flits_can_send && want_n_s ? 3'd2 : __out_s_flits_can_send && want_s_s ? 3'd3 : __out_s_flits_can_send && want_e_s ? 3'd4 : __out_s_flits_can_send && want_w_s ? 3'd5 : 3'd0;
  logic [2:0] pick_e;
  assign pick_e = __out_e_flits_can_send && want_local_e ? 3'd1 : __out_e_flits_can_send && want_n_e ? 3'd2 : __out_e_flits_can_send && want_s_e ? 3'd3 : __out_e_flits_can_send && want_e_e ? 3'd4 : __out_e_flits_can_send && want_w_e ? 3'd5 : 3'd0;
  logic [2:0] pick_w;
  assign pick_w = __out_w_flits_can_send && want_local_w ? 3'd1 : __out_w_flits_can_send && want_n_w ? 3'd2 : __out_w_flits_can_send && want_s_w ? 3'd3 : __out_w_flits_can_send && want_e_w ? 3'd4 : __out_w_flits_can_send && want_w_w ? 3'd5 : 3'd0;
  logic served_local;
  assign served_local = pick_local == 3'd1 || pick_n == 3'd1 || pick_s == 3'd1 || pick_e == 3'd1 || pick_w == 3'd1;
  logic served_n;
  assign served_n = pick_local == 3'd2 || pick_n == 3'd2 || pick_s == 3'd2 || pick_e == 3'd2 || pick_w == 3'd2;
  logic served_s;
  assign served_s = pick_local == 3'd3 || pick_n == 3'd3 || pick_s == 3'd3 || pick_e == 3'd3 || pick_w == 3'd3;
  logic served_e;
  assign served_e = pick_local == 3'd4 || pick_n == 3'd4 || pick_s == 3'd4 || pick_e == 3'd4 || pick_w == 3'd4;
  logic served_w;
  assign served_w = pick_local == 3'd5 || pick_n == 3'd5 || pick_s == 3'd5 || pick_e == 3'd5 || pick_w == 3'd5;
  assign out_local_flits_send_valid = pick_local != 3'd0;
  assign out_local_flits_send_data = pick_local == 3'd1 ? lx_data : pick_local == 3'd2 ? nx_data : pick_local == 3'd3 ? sx_data : pick_local == 3'd4 ? ex_data : wx_data;
  assign out_n_flits_send_valid = pick_n != 3'd0;
  assign out_n_flits_send_data = pick_n == 3'd1 ? lx_data : pick_n == 3'd2 ? nx_data : pick_n == 3'd3 ? sx_data : pick_n == 3'd4 ? ex_data : wx_data;
  assign out_s_flits_send_valid = pick_s != 3'd0;
  assign out_s_flits_send_data = pick_s == 3'd1 ? lx_data : pick_s == 3'd2 ? nx_data : pick_s == 3'd3 ? sx_data : pick_s == 3'd4 ? ex_data : wx_data;
  assign out_e_flits_send_valid = pick_e != 3'd0;
  assign out_e_flits_send_data = pick_e == 3'd1 ? lx_data : pick_e == 3'd2 ? nx_data : pick_e == 3'd3 ? sx_data : pick_e == 3'd4 ? ex_data : wx_data;
  assign out_w_flits_send_valid = pick_w != 3'd0;
  assign out_w_flits_send_data = pick_w == 3'd1 ? lx_data : pick_w == 3'd2 ? nx_data : pick_w == 3'd3 ? sx_data : pick_w == 3'd4 ? ex_data : wx_data;
  assign in_local_flits_credit_return = served_local;
  assign in_n_flits_credit_return = served_n;
  assign in_s_flits_credit_return = served_s;
  assign in_e_flits_credit_return = served_e;
  assign in_w_flits_credit_return = served_w;
  
  // Auto-generated credit_channel state (PR #3b-ii, sender side)
  logic [$clog2((4) + 1) - 1:0] __out_local_flits_credit;
  wire  __out_local_flits_can_send = __out_local_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_local_flits_credit <= 4;
    end else begin
      if (out_local_flits_send_valid && !out_local_flits_credit_return) __out_local_flits_credit <= __out_local_flits_credit - 1;
      else if (out_local_flits_credit_return && !out_local_flits_send_valid) __out_local_flits_credit <= __out_local_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_n_flits_credit;
  wire  __out_n_flits_can_send = __out_n_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_n_flits_credit <= 4;
    end else begin
      if (out_n_flits_send_valid && !out_n_flits_credit_return) __out_n_flits_credit <= __out_n_flits_credit - 1;
      else if (out_n_flits_credit_return && !out_n_flits_send_valid) __out_n_flits_credit <= __out_n_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_s_flits_credit;
  wire  __out_s_flits_can_send = __out_s_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_s_flits_credit <= 4;
    end else begin
      if (out_s_flits_send_valid && !out_s_flits_credit_return) __out_s_flits_credit <= __out_s_flits_credit - 1;
      else if (out_s_flits_credit_return && !out_s_flits_send_valid) __out_s_flits_credit <= __out_s_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_e_flits_credit;
  wire  __out_e_flits_can_send = __out_e_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_e_flits_credit <= 4;
    end else begin
      if (out_e_flits_send_valid && !out_e_flits_credit_return) __out_e_flits_credit <= __out_e_flits_credit - 1;
      else if (out_e_flits_credit_return && !out_e_flits_send_valid) __out_e_flits_credit <= __out_e_flits_credit + 1;
    end
  end
  logic [$clog2((4) + 1) - 1:0] __out_w_flits_credit;
  wire  __out_w_flits_can_send = __out_w_flits_credit != 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      __out_w_flits_credit <= 4;
    end else begin
      if (out_w_flits_send_valid && !out_w_flits_credit_return) __out_w_flits_credit <= __out_w_flits_credit - 1;
      else if (out_w_flits_credit_return && !out_w_flits_send_valid) __out_w_flits_credit <= __out_w_flits_credit + 1;
    end
  end
  
  // Auto-generated credit_channel target-side FIFO (PR #3b-iii)
  logic [(32) - 1:0] __in_local_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_local_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_local_flits_occ;
  wire  __in_local_flits_valid = __in_local_flits_occ != 0;
  wire [(32) - 1:0] __in_local_flits_data = __in_local_flits_buf[__in_local_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_local_flits_head <= 0;
      __in_local_flits_tail <= 0;
      __in_local_flits_occ  <= 0;
    end else begin
      if (in_local_flits_send_valid) begin
        __in_local_flits_buf[__in_local_flits_tail] <= in_local_flits_send_data;
        __in_local_flits_tail <= (__in_local_flits_tail + 1) % (4);
      end
      if ((in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_head <= (__in_local_flits_head + 1) % (4);
      if (in_local_flits_send_valid && !(in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ + 1;
      else if (!in_local_flits_send_valid &&  (in_local_flits_credit_return && __in_local_flits_valid)) __in_local_flits_occ <= __in_local_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_n_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_n_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_n_flits_occ;
  wire  __in_n_flits_valid = __in_n_flits_occ != 0;
  wire [(32) - 1:0] __in_n_flits_data = __in_n_flits_buf[__in_n_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_n_flits_head <= 0;
      __in_n_flits_tail <= 0;
      __in_n_flits_occ  <= 0;
    end else begin
      if (in_n_flits_send_valid) begin
        __in_n_flits_buf[__in_n_flits_tail] <= in_n_flits_send_data;
        __in_n_flits_tail <= (__in_n_flits_tail + 1) % (4);
      end
      if ((in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_head <= (__in_n_flits_head + 1) % (4);
      if (in_n_flits_send_valid && !(in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ + 1;
      else if (!in_n_flits_send_valid &&  (in_n_flits_credit_return && __in_n_flits_valid)) __in_n_flits_occ <= __in_n_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_s_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_s_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_s_flits_occ;
  wire  __in_s_flits_valid = __in_s_flits_occ != 0;
  wire [(32) - 1:0] __in_s_flits_data = __in_s_flits_buf[__in_s_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_s_flits_head <= 0;
      __in_s_flits_tail <= 0;
      __in_s_flits_occ  <= 0;
    end else begin
      if (in_s_flits_send_valid) begin
        __in_s_flits_buf[__in_s_flits_tail] <= in_s_flits_send_data;
        __in_s_flits_tail <= (__in_s_flits_tail + 1) % (4);
      end
      if ((in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_head <= (__in_s_flits_head + 1) % (4);
      if (in_s_flits_send_valid && !(in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ + 1;
      else if (!in_s_flits_send_valid &&  (in_s_flits_credit_return && __in_s_flits_valid)) __in_s_flits_occ <= __in_s_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_e_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_e_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_e_flits_occ;
  wire  __in_e_flits_valid = __in_e_flits_occ != 0;
  wire [(32) - 1:0] __in_e_flits_data = __in_e_flits_buf[__in_e_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_e_flits_head <= 0;
      __in_e_flits_tail <= 0;
      __in_e_flits_occ  <= 0;
    end else begin
      if (in_e_flits_send_valid) begin
        __in_e_flits_buf[__in_e_flits_tail] <= in_e_flits_send_data;
        __in_e_flits_tail <= (__in_e_flits_tail + 1) % (4);
      end
      if ((in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_head <= (__in_e_flits_head + 1) % (4);
      if (in_e_flits_send_valid && !(in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ + 1;
      else if (!in_e_flits_send_valid &&  (in_e_flits_credit_return && __in_e_flits_valid)) __in_e_flits_occ <= __in_e_flits_occ - 1;
    end
  end
  logic [(32) - 1:0] __in_w_flits_buf [(4)];
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_head;
  logic [$clog2(4) == 0 ? 0 : $clog2(4) - 1:0] __in_w_flits_tail;
  logic [$clog2((4) + 1) - 1:0] __in_w_flits_occ;
  wire  __in_w_flits_valid = __in_w_flits_occ != 0;
  wire [(32) - 1:0] __in_w_flits_data = __in_w_flits_buf[__in_w_flits_head];
  always_ff @(posedge clk) begin
    if (rst) begin
      __in_w_flits_head <= 0;
      __in_w_flits_tail <= 0;
      __in_w_flits_occ  <= 0;
    end else begin
      if (in_w_flits_send_valid) begin
        __in_w_flits_buf[__in_w_flits_tail] <= in_w_flits_send_data;
        __in_w_flits_tail <= (__in_w_flits_tail + 1) % (4);
      end
      if ((in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_head <= (__in_w_flits_head + 1) % (4);
      if (in_w_flits_send_valid && !(in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ + 1;
      else if (!in_w_flits_send_valid &&  (in_w_flits_credit_return && __in_w_flits_valid)) __in_w_flits_occ <= __in_w_flits_occ - 1;
    end
  end
  
  // synopsys translate_off
  // Auto-generated credit_channel protocol assertions (Tier 2)
  _auto_cc_out_local_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_local_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_3_Y_3._auto_cc_out_local_flits_credit_bounds");
  _auto_cc_out_local_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_local_flits_send_valid |-> __out_local_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_3_Y_3._auto_cc_out_local_flits_send_requires_credit");
  _auto_cc_out_n_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_n_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_3_Y_3._auto_cc_out_n_flits_credit_bounds");
  _auto_cc_out_n_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_n_flits_send_valid |-> __out_n_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_3_Y_3._auto_cc_out_n_flits_send_requires_credit");
  _auto_cc_out_s_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_s_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_3_Y_3._auto_cc_out_s_flits_credit_bounds");
  _auto_cc_out_s_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_s_flits_send_valid |-> __out_s_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_3_Y_3._auto_cc_out_s_flits_send_requires_credit");
  _auto_cc_out_e_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_e_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_3_Y_3._auto_cc_out_e_flits_credit_bounds");
  _auto_cc_out_e_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_e_flits_send_valid |-> __out_e_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_3_Y_3._auto_cc_out_e_flits_send_requires_credit");
  _auto_cc_out_w_flits_credit_bounds: assert property (@(posedge clk) disable iff (rst) __out_w_flits_credit <= (4))
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit exceeds DEPTH): Router__X_3_Y_3._auto_cc_out_w_flits_credit_bounds");
  _auto_cc_out_w_flits_send_requires_credit: assert property (@(posedge clk) disable iff (rst) out_w_flits_send_valid |-> __out_w_flits_credit > 0)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (send without credit): Router__X_3_Y_3._auto_cc_out_w_flits_send_requires_credit");
  _auto_cc_in_local_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_local_flits_credit_return |-> __in_local_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_3_Y_3._auto_cc_in_local_flits_credit_return_requires_buffered");
  _auto_cc_in_n_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_n_flits_credit_return |-> __in_n_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_3_Y_3._auto_cc_in_n_flits_credit_return_requires_buffered");
  _auto_cc_in_s_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_s_flits_credit_return |-> __in_s_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_3_Y_3._auto_cc_in_s_flits_credit_return_requires_buffered");
  _auto_cc_in_e_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_e_flits_credit_return |-> __in_e_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_3_Y_3._auto_cc_in_e_flits_credit_return_requires_buffered");
  _auto_cc_in_w_flits_credit_return_requires_buffered: assert property (@(posedge clk) disable iff (rst) in_w_flits_credit_return |-> __in_w_flits_valid)
    else $fatal(1, "CREDIT-CHANNEL VIOLATION (credit_return without buffered data): Router__X_3_Y_3._auto_cc_in_w_flits_credit_return_requires_buffered");
  // synopsys translate_on

endmodule

// Per-output drives: pick the winner's data; otherwise idle.
// Pop each input that was served somewhere.
// 4×4 mesh NoC top — instantiates 16 Router(X, Y) instances and
// wires them with internal credit_channel links + self-loop tie-offs
// for edge directions (router routes are bounded to in-grid dests so
// the tied-off output never sees a send and its credit stays full).
module FlitProducer (
  input logic clk,
  input logic rst,
  input logic [7:0] gen_pressure,
  input logic [1:0] dst_x,
  input logic [1:0] dst_y,
  output logic out_flits_send_valid,
  output logic [31:0] out_flits_send_data,
  input logic out_flits_credit_return
);

  logic [27:0] seq_no;
  logic [7:0] lfsr;
  always_comb begin
    if (1'd1) begin
      out_flits_send_valid = 1'd0;
      out_flits_send_data = 0;
    end
    if (__out_flits_can_send && lfsr < gen_pressure) begin
      if (1'd1) begin
        out_flits_send_valid = 1'd1;
        out_flits_send_data = {seq_no, dst_y, dst_x};
      end
    end
  end
  always_ff @(posedge clk) begin
    if (rst) begin
      lfsr <= 8'd90;
      seq_no <= 28'd0;
    end else begin
      if (lfsr[0]) begin
        lfsr <= lfsr >> 1 ^ 8'd184;
      end else begin
        lfsr <= lfsr >> 1;
      end
      if (__out_flits_can_send && lfsr < gen_pressure) begin
        seq_no <= 28'(seq_no + 28'd1);
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

module FlitConsumer (
  input logic clk,
  input logic rst,
  input logic [7:0] pop_pressure,
  input logic incoming_flits_send_valid,
  input logic [31:0] incoming_flits_send_data,
  output logic incoming_flits_credit_return,
  output logic [31:0] popped_count,
  output logic [27:0] last_payload
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
      last_payload <= 28'd0;
      lfsr <= 8'd195;
      popped_count <= 32'd0;
    end else begin
      if (lfsr[0]) begin
        lfsr <= lfsr >> 1 ^ 8'd184;
      end else begin
        lfsr <= lfsr >> 1;
      end
      if (__incoming_flits_valid && lfsr < pop_pressure) begin
        popped_count <= 32'(popped_count + 32'd1);
        last_payload <= __incoming_flits_data[31:4];
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

module Mesh4x4 (
  input logic clk,
  input logic rst,
  input logic [7:0] gen_pressure,
  input logic [7:0] pop_pressure,
  input logic [1:0] dst_x,
  input logic [1:0] dst_y,
  output logic [31:0] popped_count,
  output logic [27:0] last_payload
);

  // Producer at (0,0) local, consumer at (3,3) local.
  logic inj_link_flits_send_valid;
  logic [31:0] inj_link_flits_send_data;
  logic inj_link_flits_credit_return;
  FlitProducer prod (
    .clk(clk),
    .rst(rst),
    .gen_pressure(gen_pressure),
    .dst_x(dst_x),
    .dst_y(dst_y),
    .out_flits_send_valid(inj_link_flits_send_valid),
    .out_flits_send_data(inj_link_flits_send_data),
    .out_flits_credit_return(inj_link_flits_credit_return)
  );
  logic snk_link_flits_send_valid;
  logic [31:0] snk_link_flits_send_data;
  logic snk_link_flits_credit_return;
  FlitConsumer cons (
    .clk(clk),
    .rst(rst),
    .pop_pressure(pop_pressure),
    .incoming_flits_send_valid(snk_link_flits_send_valid),
    .incoming_flits_send_data(snk_link_flits_send_data),
    .incoming_flits_credit_return(snk_link_flits_credit_return),
    .popped_count(popped_count),
    .last_payload(last_payload)
  );
  logic tie_l_0_0_flits_send_valid;
  logic [31:0] tie_l_0_0_flits_send_data;
  logic tie_l_0_0_flits_credit_return;
  logic n2s_0_0_flits_send_valid;
  logic [31:0] n2s_0_0_flits_send_data;
  logic n2s_0_0_flits_credit_return;
  logic s2n_0_0_flits_send_valid;
  logic [31:0] s2n_0_0_flits_send_data;
  logic s2n_0_0_flits_credit_return;
  logic tie_s_0_0_flits_send_valid;
  logic [31:0] tie_s_0_0_flits_send_data;
  logic tie_s_0_0_flits_credit_return;
  logic e2w_0_0_flits_send_valid;
  logic [31:0] e2w_0_0_flits_send_data;
  logic e2w_0_0_flits_credit_return;
  logic w2e_0_0_flits_send_valid;
  logic [31:0] w2e_0_0_flits_send_data;
  logic w2e_0_0_flits_credit_return;
  logic tie_w_0_0_flits_send_valid;
  logic [31:0] tie_w_0_0_flits_send_data;
  logic tie_w_0_0_flits_credit_return;
  Router__X_0_Y_0 #(.X(0), .Y(0)) r_0_0 (
    .clk(clk),
    .rst(rst),
    .in_local_flits_send_valid(inj_link_flits_send_valid),
    .in_local_flits_send_data(inj_link_flits_send_data),
    .in_local_flits_credit_return(inj_link_flits_credit_return),
    .out_local_flits_send_valid(tie_l_0_0_flits_send_valid),
    .out_local_flits_send_data(tie_l_0_0_flits_send_data),
    .out_local_flits_credit_return(tie_l_0_0_flits_credit_return),
    .in_n_flits_send_valid(n2s_0_0_flits_send_valid),
    .in_n_flits_send_data(n2s_0_0_flits_send_data),
    .in_n_flits_credit_return(n2s_0_0_flits_credit_return),
    .out_n_flits_send_valid(s2n_0_0_flits_send_valid),
    .out_n_flits_send_data(s2n_0_0_flits_send_data),
    .out_n_flits_credit_return(s2n_0_0_flits_credit_return),
    .in_s_flits_send_valid(tie_s_0_0_flits_send_valid),
    .in_s_flits_send_data(tie_s_0_0_flits_send_data),
    .in_s_flits_credit_return(tie_s_0_0_flits_credit_return),
    .out_s_flits_send_valid(tie_s_0_0_flits_send_valid),
    .out_s_flits_send_data(tie_s_0_0_flits_send_data),
    .out_s_flits_credit_return(tie_s_0_0_flits_credit_return),
    .in_e_flits_send_valid(e2w_0_0_flits_send_valid),
    .in_e_flits_send_data(e2w_0_0_flits_send_data),
    .in_e_flits_credit_return(e2w_0_0_flits_credit_return),
    .out_e_flits_send_valid(w2e_0_0_flits_send_valid),
    .out_e_flits_send_data(w2e_0_0_flits_send_data),
    .out_e_flits_credit_return(w2e_0_0_flits_credit_return),
    .in_w_flits_send_valid(tie_w_0_0_flits_send_valid),
    .in_w_flits_send_data(tie_w_0_0_flits_send_data),
    .in_w_flits_credit_return(tie_w_0_0_flits_credit_return),
    .out_w_flits_send_valid(tie_w_0_0_flits_send_valid),
    .out_w_flits_send_data(tie_w_0_0_flits_send_data),
    .out_w_flits_credit_return(tie_w_0_0_flits_credit_return)
  );
  logic tie_l_1_0_flits_send_valid;
  logic [31:0] tie_l_1_0_flits_send_data;
  logic tie_l_1_0_flits_credit_return;
  logic n2s_1_0_flits_send_valid;
  logic [31:0] n2s_1_0_flits_send_data;
  logic n2s_1_0_flits_credit_return;
  logic s2n_1_0_flits_send_valid;
  logic [31:0] s2n_1_0_flits_send_data;
  logic s2n_1_0_flits_credit_return;
  logic tie_s_1_0_flits_send_valid;
  logic [31:0] tie_s_1_0_flits_send_data;
  logic tie_s_1_0_flits_credit_return;
  logic e2w_1_0_flits_send_valid;
  logic [31:0] e2w_1_0_flits_send_data;
  logic e2w_1_0_flits_credit_return;
  logic w2e_1_0_flits_send_valid;
  logic [31:0] w2e_1_0_flits_send_data;
  logic w2e_1_0_flits_credit_return;
  Router__X_1_Y_0 #(.X(1), .Y(0)) r_1_0 (
    .clk(clk),
    .rst(rst),
    .in_local_flits_send_valid(tie_l_1_0_flits_send_valid),
    .in_local_flits_send_data(tie_l_1_0_flits_send_data),
    .in_local_flits_credit_return(tie_l_1_0_flits_credit_return),
    .out_local_flits_send_valid(tie_l_1_0_flits_send_valid),
    .out_local_flits_send_data(tie_l_1_0_flits_send_data),
    .out_local_flits_credit_return(tie_l_1_0_flits_credit_return),
    .in_n_flits_send_valid(n2s_1_0_flits_send_valid),
    .in_n_flits_send_data(n2s_1_0_flits_send_data),
    .in_n_flits_credit_return(n2s_1_0_flits_credit_return),
    .out_n_flits_send_valid(s2n_1_0_flits_send_valid),
    .out_n_flits_send_data(s2n_1_0_flits_send_data),
    .out_n_flits_credit_return(s2n_1_0_flits_credit_return),
    .in_s_flits_send_valid(tie_s_1_0_flits_send_valid),
    .in_s_flits_send_data(tie_s_1_0_flits_send_data),
    .in_s_flits_credit_return(tie_s_1_0_flits_credit_return),
    .out_s_flits_send_valid(tie_s_1_0_flits_send_valid),
    .out_s_flits_send_data(tie_s_1_0_flits_send_data),
    .out_s_flits_credit_return(tie_s_1_0_flits_credit_return),
    .in_e_flits_send_valid(e2w_1_0_flits_send_valid),
    .in_e_flits_send_data(e2w_1_0_flits_send_data),
    .in_e_flits_credit_return(e2w_1_0_flits_credit_return),
    .out_e_flits_send_valid(w2e_1_0_flits_send_valid),
    .out_e_flits_send_data(w2e_1_0_flits_send_data),
    .out_e_flits_credit_return(w2e_1_0_flits_credit_return),
    .in_w_flits_send_valid(w2e_0_0_flits_send_valid),
    .in_w_flits_send_data(w2e_0_0_flits_send_data),
    .in_w_flits_credit_return(w2e_0_0_flits_credit_return),
    .out_w_flits_send_valid(e2w_0_0_flits_send_valid),
    .out_w_flits_send_data(e2w_0_0_flits_send_data),
    .out_w_flits_credit_return(e2w_0_0_flits_credit_return)
  );
  logic tie_l_2_0_flits_send_valid;
  logic [31:0] tie_l_2_0_flits_send_data;
  logic tie_l_2_0_flits_credit_return;
  logic n2s_2_0_flits_send_valid;
  logic [31:0] n2s_2_0_flits_send_data;
  logic n2s_2_0_flits_credit_return;
  logic s2n_2_0_flits_send_valid;
  logic [31:0] s2n_2_0_flits_send_data;
  logic s2n_2_0_flits_credit_return;
  logic tie_s_2_0_flits_send_valid;
  logic [31:0] tie_s_2_0_flits_send_data;
  logic tie_s_2_0_flits_credit_return;
  logic e2w_2_0_flits_send_valid;
  logic [31:0] e2w_2_0_flits_send_data;
  logic e2w_2_0_flits_credit_return;
  logic w2e_2_0_flits_send_valid;
  logic [31:0] w2e_2_0_flits_send_data;
  logic w2e_2_0_flits_credit_return;
  Router__X_2_Y_0 #(.X(2), .Y(0)) r_2_0 (
    .clk(clk),
    .rst(rst),
    .in_local_flits_send_valid(tie_l_2_0_flits_send_valid),
    .in_local_flits_send_data(tie_l_2_0_flits_send_data),
    .in_local_flits_credit_return(tie_l_2_0_flits_credit_return),
    .out_local_flits_send_valid(tie_l_2_0_flits_send_valid),
    .out_local_flits_send_data(tie_l_2_0_flits_send_data),
    .out_local_flits_credit_return(tie_l_2_0_flits_credit_return),
    .in_n_flits_send_valid(n2s_2_0_flits_send_valid),
    .in_n_flits_send_data(n2s_2_0_flits_send_data),
    .in_n_flits_credit_return(n2s_2_0_flits_credit_return),
    .out_n_flits_send_valid(s2n_2_0_flits_send_valid),
    .out_n_flits_send_data(s2n_2_0_flits_send_data),
    .out_n_flits_credit_return(s2n_2_0_flits_credit_return),
    .in_s_flits_send_valid(tie_s_2_0_flits_send_valid),
    .in_s_flits_send_data(tie_s_2_0_flits_send_data),
    .in_s_flits_credit_return(tie_s_2_0_flits_credit_return),
    .out_s_flits_send_valid(tie_s_2_0_flits_send_valid),
    .out_s_flits_send_data(tie_s_2_0_flits_send_data),
    .out_s_flits_credit_return(tie_s_2_0_flits_credit_return),
    .in_e_flits_send_valid(e2w_2_0_flits_send_valid),
    .in_e_flits_send_data(e2w_2_0_flits_send_data),
    .in_e_flits_credit_return(e2w_2_0_flits_credit_return),
    .out_e_flits_send_valid(w2e_2_0_flits_send_valid),
    .out_e_flits_send_data(w2e_2_0_flits_send_data),
    .out_e_flits_credit_return(w2e_2_0_flits_credit_return),
    .in_w_flits_send_valid(w2e_1_0_flits_send_valid),
    .in_w_flits_send_data(w2e_1_0_flits_send_data),
    .in_w_flits_credit_return(w2e_1_0_flits_credit_return),
    .out_w_flits_send_valid(e2w_1_0_flits_send_valid),
    .out_w_flits_send_data(e2w_1_0_flits_send_data),
    .out_w_flits_credit_return(e2w_1_0_flits_credit_return)
  );
  logic tie_l_3_0_flits_send_valid;
  logic [31:0] tie_l_3_0_flits_send_data;
  logic tie_l_3_0_flits_credit_return;
  logic n2s_3_0_flits_send_valid;
  logic [31:0] n2s_3_0_flits_send_data;
  logic n2s_3_0_flits_credit_return;
  logic s2n_3_0_flits_send_valid;
  logic [31:0] s2n_3_0_flits_send_data;
  logic s2n_3_0_flits_credit_return;
  logic tie_s_3_0_flits_send_valid;
  logic [31:0] tie_s_3_0_flits_send_data;
  logic tie_s_3_0_flits_credit_return;
  logic tie_e_3_0_flits_send_valid;
  logic [31:0] tie_e_3_0_flits_send_data;
  logic tie_e_3_0_flits_credit_return;
  Router__X_3_Y_0 #(.X(3), .Y(0)) r_3_0 (
    .clk(clk),
    .rst(rst),
    .in_local_flits_send_valid(tie_l_3_0_flits_send_valid),
    .in_local_flits_send_data(tie_l_3_0_flits_send_data),
    .in_local_flits_credit_return(tie_l_3_0_flits_credit_return),
    .out_local_flits_send_valid(tie_l_3_0_flits_send_valid),
    .out_local_flits_send_data(tie_l_3_0_flits_send_data),
    .out_local_flits_credit_return(tie_l_3_0_flits_credit_return),
    .in_n_flits_send_valid(n2s_3_0_flits_send_valid),
    .in_n_flits_send_data(n2s_3_0_flits_send_data),
    .in_n_flits_credit_return(n2s_3_0_flits_credit_return),
    .out_n_flits_send_valid(s2n_3_0_flits_send_valid),
    .out_n_flits_send_data(s2n_3_0_flits_send_data),
    .out_n_flits_credit_return(s2n_3_0_flits_credit_return),
    .in_s_flits_send_valid(tie_s_3_0_flits_send_valid),
    .in_s_flits_send_data(tie_s_3_0_flits_send_data),
    .in_s_flits_credit_return(tie_s_3_0_flits_credit_return),
    .out_s_flits_send_valid(tie_s_3_0_flits_send_valid),
    .out_s_flits_send_data(tie_s_3_0_flits_send_data),
    .out_s_flits_credit_return(tie_s_3_0_flits_credit_return),
    .in_e_flits_send_valid(tie_e_3_0_flits_send_valid),
    .in_e_flits_send_data(tie_e_3_0_flits_send_data),
    .in_e_flits_credit_return(tie_e_3_0_flits_credit_return),
    .out_e_flits_send_valid(tie_e_3_0_flits_send_valid),
    .out_e_flits_send_data(tie_e_3_0_flits_send_data),
    .out_e_flits_credit_return(tie_e_3_0_flits_credit_return),
    .in_w_flits_send_valid(w2e_2_0_flits_send_valid),
    .in_w_flits_send_data(w2e_2_0_flits_send_data),
    .in_w_flits_credit_return(w2e_2_0_flits_credit_return),
    .out_w_flits_send_valid(e2w_2_0_flits_send_valid),
    .out_w_flits_send_data(e2w_2_0_flits_send_data),
    .out_w_flits_credit_return(e2w_2_0_flits_credit_return)
  );
  logic tie_l_0_1_flits_send_valid;
  logic [31:0] tie_l_0_1_flits_send_data;
  logic tie_l_0_1_flits_credit_return;
  logic n2s_0_1_flits_send_valid;
  logic [31:0] n2s_0_1_flits_send_data;
  logic n2s_0_1_flits_credit_return;
  logic s2n_0_1_flits_send_valid;
  logic [31:0] s2n_0_1_flits_send_data;
  logic s2n_0_1_flits_credit_return;
  logic e2w_0_1_flits_send_valid;
  logic [31:0] e2w_0_1_flits_send_data;
  logic e2w_0_1_flits_credit_return;
  logic w2e_0_1_flits_send_valid;
  logic [31:0] w2e_0_1_flits_send_data;
  logic w2e_0_1_flits_credit_return;
  logic tie_w_0_1_flits_send_valid;
  logic [31:0] tie_w_0_1_flits_send_data;
  logic tie_w_0_1_flits_credit_return;
  Router__X_0_Y_1 #(.X(0), .Y(1)) r_0_1 (
    .clk(clk),
    .rst(rst),
    .in_local_flits_send_valid(tie_l_0_1_flits_send_valid),
    .in_local_flits_send_data(tie_l_0_1_flits_send_data),
    .in_local_flits_credit_return(tie_l_0_1_flits_credit_return),
    .out_local_flits_send_valid(tie_l_0_1_flits_send_valid),
    .out_local_flits_send_data(tie_l_0_1_flits_send_data),
    .out_local_flits_credit_return(tie_l_0_1_flits_credit_return),
    .in_n_flits_send_valid(n2s_0_1_flits_send_valid),
    .in_n_flits_send_data(n2s_0_1_flits_send_data),
    .in_n_flits_credit_return(n2s_0_1_flits_credit_return),
    .out_n_flits_send_valid(s2n_0_1_flits_send_valid),
    .out_n_flits_send_data(s2n_0_1_flits_send_data),
    .out_n_flits_credit_return(s2n_0_1_flits_credit_return),
    .in_s_flits_send_valid(s2n_0_0_flits_send_valid),
    .in_s_flits_send_data(s2n_0_0_flits_send_data),
    .in_s_flits_credit_return(s2n_0_0_flits_credit_return),
    .out_s_flits_send_valid(n2s_0_0_flits_send_valid),
    .out_s_flits_send_data(n2s_0_0_flits_send_data),
    .out_s_flits_credit_return(n2s_0_0_flits_credit_return),
    .in_e_flits_send_valid(e2w_0_1_flits_send_valid),
    .in_e_flits_send_data(e2w_0_1_flits_send_data),
    .in_e_flits_credit_return(e2w_0_1_flits_credit_return),
    .out_e_flits_send_valid(w2e_0_1_flits_send_valid),
    .out_e_flits_send_data(w2e_0_1_flits_send_data),
    .out_e_flits_credit_return(w2e_0_1_flits_credit_return),
    .in_w_flits_send_valid(tie_w_0_1_flits_send_valid),
    .in_w_flits_send_data(tie_w_0_1_flits_send_data),
    .in_w_flits_credit_return(tie_w_0_1_flits_credit_return),
    .out_w_flits_send_valid(tie_w_0_1_flits_send_valid),
    .out_w_flits_send_data(tie_w_0_1_flits_send_data),
    .out_w_flits_credit_return(tie_w_0_1_flits_credit_return)
  );
  logic tie_l_1_1_flits_send_valid;
  logic [31:0] tie_l_1_1_flits_send_data;
  logic tie_l_1_1_flits_credit_return;
  logic n2s_1_1_flits_send_valid;
  logic [31:0] n2s_1_1_flits_send_data;
  logic n2s_1_1_flits_credit_return;
  logic s2n_1_1_flits_send_valid;
  logic [31:0] s2n_1_1_flits_send_data;
  logic s2n_1_1_flits_credit_return;
  logic e2w_1_1_flits_send_valid;
  logic [31:0] e2w_1_1_flits_send_data;
  logic e2w_1_1_flits_credit_return;
  logic w2e_1_1_flits_send_valid;
  logic [31:0] w2e_1_1_flits_send_data;
  logic w2e_1_1_flits_credit_return;
  Router__X_1_Y_1 #(.X(1), .Y(1)) r_1_1 (
    .clk(clk),
    .rst(rst),
    .in_local_flits_send_valid(tie_l_1_1_flits_send_valid),
    .in_local_flits_send_data(tie_l_1_1_flits_send_data),
    .in_local_flits_credit_return(tie_l_1_1_flits_credit_return),
    .out_local_flits_send_valid(tie_l_1_1_flits_send_valid),
    .out_local_flits_send_data(tie_l_1_1_flits_send_data),
    .out_local_flits_credit_return(tie_l_1_1_flits_credit_return),
    .in_n_flits_send_valid(n2s_1_1_flits_send_valid),
    .in_n_flits_send_data(n2s_1_1_flits_send_data),
    .in_n_flits_credit_return(n2s_1_1_flits_credit_return),
    .out_n_flits_send_valid(s2n_1_1_flits_send_valid),
    .out_n_flits_send_data(s2n_1_1_flits_send_data),
    .out_n_flits_credit_return(s2n_1_1_flits_credit_return),
    .in_s_flits_send_valid(s2n_1_0_flits_send_valid),
    .in_s_flits_send_data(s2n_1_0_flits_send_data),
    .in_s_flits_credit_return(s2n_1_0_flits_credit_return),
    .out_s_flits_send_valid(n2s_1_0_flits_send_valid),
    .out_s_flits_send_data(n2s_1_0_flits_send_data),
    .out_s_flits_credit_return(n2s_1_0_flits_credit_return),
    .in_e_flits_send_valid(e2w_1_1_flits_send_valid),
    .in_e_flits_send_data(e2w_1_1_flits_send_data),
    .in_e_flits_credit_return(e2w_1_1_flits_credit_return),
    .out_e_flits_send_valid(w2e_1_1_flits_send_valid),
    .out_e_flits_send_data(w2e_1_1_flits_send_data),
    .out_e_flits_credit_return(w2e_1_1_flits_credit_return),
    .in_w_flits_send_valid(w2e_0_1_flits_send_valid),
    .in_w_flits_send_data(w2e_0_1_flits_send_data),
    .in_w_flits_credit_return(w2e_0_1_flits_credit_return),
    .out_w_flits_send_valid(e2w_0_1_flits_send_valid),
    .out_w_flits_send_data(e2w_0_1_flits_send_data),
    .out_w_flits_credit_return(e2w_0_1_flits_credit_return)
  );
  logic tie_l_2_1_flits_send_valid;
  logic [31:0] tie_l_2_1_flits_send_data;
  logic tie_l_2_1_flits_credit_return;
  logic n2s_2_1_flits_send_valid;
  logic [31:0] n2s_2_1_flits_send_data;
  logic n2s_2_1_flits_credit_return;
  logic s2n_2_1_flits_send_valid;
  logic [31:0] s2n_2_1_flits_send_data;
  logic s2n_2_1_flits_credit_return;
  logic e2w_2_1_flits_send_valid;
  logic [31:0] e2w_2_1_flits_send_data;
  logic e2w_2_1_flits_credit_return;
  logic w2e_2_1_flits_send_valid;
  logic [31:0] w2e_2_1_flits_send_data;
  logic w2e_2_1_flits_credit_return;
  Router__X_2_Y_1 #(.X(2), .Y(1)) r_2_1 (
    .clk(clk),
    .rst(rst),
    .in_local_flits_send_valid(tie_l_2_1_flits_send_valid),
    .in_local_flits_send_data(tie_l_2_1_flits_send_data),
    .in_local_flits_credit_return(tie_l_2_1_flits_credit_return),
    .out_local_flits_send_valid(tie_l_2_1_flits_send_valid),
    .out_local_flits_send_data(tie_l_2_1_flits_send_data),
    .out_local_flits_credit_return(tie_l_2_1_flits_credit_return),
    .in_n_flits_send_valid(n2s_2_1_flits_send_valid),
    .in_n_flits_send_data(n2s_2_1_flits_send_data),
    .in_n_flits_credit_return(n2s_2_1_flits_credit_return),
    .out_n_flits_send_valid(s2n_2_1_flits_send_valid),
    .out_n_flits_send_data(s2n_2_1_flits_send_data),
    .out_n_flits_credit_return(s2n_2_1_flits_credit_return),
    .in_s_flits_send_valid(s2n_2_0_flits_send_valid),
    .in_s_flits_send_data(s2n_2_0_flits_send_data),
    .in_s_flits_credit_return(s2n_2_0_flits_credit_return),
    .out_s_flits_send_valid(n2s_2_0_flits_send_valid),
    .out_s_flits_send_data(n2s_2_0_flits_send_data),
    .out_s_flits_credit_return(n2s_2_0_flits_credit_return),
    .in_e_flits_send_valid(e2w_2_1_flits_send_valid),
    .in_e_flits_send_data(e2w_2_1_flits_send_data),
    .in_e_flits_credit_return(e2w_2_1_flits_credit_return),
    .out_e_flits_send_valid(w2e_2_1_flits_send_valid),
    .out_e_flits_send_data(w2e_2_1_flits_send_data),
    .out_e_flits_credit_return(w2e_2_1_flits_credit_return),
    .in_w_flits_send_valid(w2e_1_1_flits_send_valid),
    .in_w_flits_send_data(w2e_1_1_flits_send_data),
    .in_w_flits_credit_return(w2e_1_1_flits_credit_return),
    .out_w_flits_send_valid(e2w_1_1_flits_send_valid),
    .out_w_flits_send_data(e2w_1_1_flits_send_data),
    .out_w_flits_credit_return(e2w_1_1_flits_credit_return)
  );
  logic tie_l_3_1_flits_send_valid;
  logic [31:0] tie_l_3_1_flits_send_data;
  logic tie_l_3_1_flits_credit_return;
  logic n2s_3_1_flits_send_valid;
  logic [31:0] n2s_3_1_flits_send_data;
  logic n2s_3_1_flits_credit_return;
  logic s2n_3_1_flits_send_valid;
  logic [31:0] s2n_3_1_flits_send_data;
  logic s2n_3_1_flits_credit_return;
  logic tie_e_3_1_flits_send_valid;
  logic [31:0] tie_e_3_1_flits_send_data;
  logic tie_e_3_1_flits_credit_return;
  Router__X_3_Y_1 #(.X(3), .Y(1)) r_3_1 (
    .clk(clk),
    .rst(rst),
    .in_local_flits_send_valid(tie_l_3_1_flits_send_valid),
    .in_local_flits_send_data(tie_l_3_1_flits_send_data),
    .in_local_flits_credit_return(tie_l_3_1_flits_credit_return),
    .out_local_flits_send_valid(tie_l_3_1_flits_send_valid),
    .out_local_flits_send_data(tie_l_3_1_flits_send_data),
    .out_local_flits_credit_return(tie_l_3_1_flits_credit_return),
    .in_n_flits_send_valid(n2s_3_1_flits_send_valid),
    .in_n_flits_send_data(n2s_3_1_flits_send_data),
    .in_n_flits_credit_return(n2s_3_1_flits_credit_return),
    .out_n_flits_send_valid(s2n_3_1_flits_send_valid),
    .out_n_flits_send_data(s2n_3_1_flits_send_data),
    .out_n_flits_credit_return(s2n_3_1_flits_credit_return),
    .in_s_flits_send_valid(s2n_3_0_flits_send_valid),
    .in_s_flits_send_data(s2n_3_0_flits_send_data),
    .in_s_flits_credit_return(s2n_3_0_flits_credit_return),
    .out_s_flits_send_valid(n2s_3_0_flits_send_valid),
    .out_s_flits_send_data(n2s_3_0_flits_send_data),
    .out_s_flits_credit_return(n2s_3_0_flits_credit_return),
    .in_e_flits_send_valid(tie_e_3_1_flits_send_valid),
    .in_e_flits_send_data(tie_e_3_1_flits_send_data),
    .in_e_flits_credit_return(tie_e_3_1_flits_credit_return),
    .out_e_flits_send_valid(tie_e_3_1_flits_send_valid),
    .out_e_flits_send_data(tie_e_3_1_flits_send_data),
    .out_e_flits_credit_return(tie_e_3_1_flits_credit_return),
    .in_w_flits_send_valid(w2e_2_1_flits_send_valid),
    .in_w_flits_send_data(w2e_2_1_flits_send_data),
    .in_w_flits_credit_return(w2e_2_1_flits_credit_return),
    .out_w_flits_send_valid(e2w_2_1_flits_send_valid),
    .out_w_flits_send_data(e2w_2_1_flits_send_data),
    .out_w_flits_credit_return(e2w_2_1_flits_credit_return)
  );
  logic tie_l_0_2_flits_send_valid;
  logic [31:0] tie_l_0_2_flits_send_data;
  logic tie_l_0_2_flits_credit_return;
  logic n2s_0_2_flits_send_valid;
  logic [31:0] n2s_0_2_flits_send_data;
  logic n2s_0_2_flits_credit_return;
  logic s2n_0_2_flits_send_valid;
  logic [31:0] s2n_0_2_flits_send_data;
  logic s2n_0_2_flits_credit_return;
  logic e2w_0_2_flits_send_valid;
  logic [31:0] e2w_0_2_flits_send_data;
  logic e2w_0_2_flits_credit_return;
  logic w2e_0_2_flits_send_valid;
  logic [31:0] w2e_0_2_flits_send_data;
  logic w2e_0_2_flits_credit_return;
  logic tie_w_0_2_flits_send_valid;
  logic [31:0] tie_w_0_2_flits_send_data;
  logic tie_w_0_2_flits_credit_return;
  Router__X_0_Y_2 #(.X(0), .Y(2)) r_0_2 (
    .clk(clk),
    .rst(rst),
    .in_local_flits_send_valid(tie_l_0_2_flits_send_valid),
    .in_local_flits_send_data(tie_l_0_2_flits_send_data),
    .in_local_flits_credit_return(tie_l_0_2_flits_credit_return),
    .out_local_flits_send_valid(tie_l_0_2_flits_send_valid),
    .out_local_flits_send_data(tie_l_0_2_flits_send_data),
    .out_local_flits_credit_return(tie_l_0_2_flits_credit_return),
    .in_n_flits_send_valid(n2s_0_2_flits_send_valid),
    .in_n_flits_send_data(n2s_0_2_flits_send_data),
    .in_n_flits_credit_return(n2s_0_2_flits_credit_return),
    .out_n_flits_send_valid(s2n_0_2_flits_send_valid),
    .out_n_flits_send_data(s2n_0_2_flits_send_data),
    .out_n_flits_credit_return(s2n_0_2_flits_credit_return),
    .in_s_flits_send_valid(s2n_0_1_flits_send_valid),
    .in_s_flits_send_data(s2n_0_1_flits_send_data),
    .in_s_flits_credit_return(s2n_0_1_flits_credit_return),
    .out_s_flits_send_valid(n2s_0_1_flits_send_valid),
    .out_s_flits_send_data(n2s_0_1_flits_send_data),
    .out_s_flits_credit_return(n2s_0_1_flits_credit_return),
    .in_e_flits_send_valid(e2w_0_2_flits_send_valid),
    .in_e_flits_send_data(e2w_0_2_flits_send_data),
    .in_e_flits_credit_return(e2w_0_2_flits_credit_return),
    .out_e_flits_send_valid(w2e_0_2_flits_send_valid),
    .out_e_flits_send_data(w2e_0_2_flits_send_data),
    .out_e_flits_credit_return(w2e_0_2_flits_credit_return),
    .in_w_flits_send_valid(tie_w_0_2_flits_send_valid),
    .in_w_flits_send_data(tie_w_0_2_flits_send_data),
    .in_w_flits_credit_return(tie_w_0_2_flits_credit_return),
    .out_w_flits_send_valid(tie_w_0_2_flits_send_valid),
    .out_w_flits_send_data(tie_w_0_2_flits_send_data),
    .out_w_flits_credit_return(tie_w_0_2_flits_credit_return)
  );
  logic tie_l_1_2_flits_send_valid;
  logic [31:0] tie_l_1_2_flits_send_data;
  logic tie_l_1_2_flits_credit_return;
  logic n2s_1_2_flits_send_valid;
  logic [31:0] n2s_1_2_flits_send_data;
  logic n2s_1_2_flits_credit_return;
  logic s2n_1_2_flits_send_valid;
  logic [31:0] s2n_1_2_flits_send_data;
  logic s2n_1_2_flits_credit_return;
  logic e2w_1_2_flits_send_valid;
  logic [31:0] e2w_1_2_flits_send_data;
  logic e2w_1_2_flits_credit_return;
  logic w2e_1_2_flits_send_valid;
  logic [31:0] w2e_1_2_flits_send_data;
  logic w2e_1_2_flits_credit_return;
  Router__X_1_Y_2 #(.X(1), .Y(2)) r_1_2 (
    .clk(clk),
    .rst(rst),
    .in_local_flits_send_valid(tie_l_1_2_flits_send_valid),
    .in_local_flits_send_data(tie_l_1_2_flits_send_data),
    .in_local_flits_credit_return(tie_l_1_2_flits_credit_return),
    .out_local_flits_send_valid(tie_l_1_2_flits_send_valid),
    .out_local_flits_send_data(tie_l_1_2_flits_send_data),
    .out_local_flits_credit_return(tie_l_1_2_flits_credit_return),
    .in_n_flits_send_valid(n2s_1_2_flits_send_valid),
    .in_n_flits_send_data(n2s_1_2_flits_send_data),
    .in_n_flits_credit_return(n2s_1_2_flits_credit_return),
    .out_n_flits_send_valid(s2n_1_2_flits_send_valid),
    .out_n_flits_send_data(s2n_1_2_flits_send_data),
    .out_n_flits_credit_return(s2n_1_2_flits_credit_return),
    .in_s_flits_send_valid(s2n_1_1_flits_send_valid),
    .in_s_flits_send_data(s2n_1_1_flits_send_data),
    .in_s_flits_credit_return(s2n_1_1_flits_credit_return),
    .out_s_flits_send_valid(n2s_1_1_flits_send_valid),
    .out_s_flits_send_data(n2s_1_1_flits_send_data),
    .out_s_flits_credit_return(n2s_1_1_flits_credit_return),
    .in_e_flits_send_valid(e2w_1_2_flits_send_valid),
    .in_e_flits_send_data(e2w_1_2_flits_send_data),
    .in_e_flits_credit_return(e2w_1_2_flits_credit_return),
    .out_e_flits_send_valid(w2e_1_2_flits_send_valid),
    .out_e_flits_send_data(w2e_1_2_flits_send_data),
    .out_e_flits_credit_return(w2e_1_2_flits_credit_return),
    .in_w_flits_send_valid(w2e_0_2_flits_send_valid),
    .in_w_flits_send_data(w2e_0_2_flits_send_data),
    .in_w_flits_credit_return(w2e_0_2_flits_credit_return),
    .out_w_flits_send_valid(e2w_0_2_flits_send_valid),
    .out_w_flits_send_data(e2w_0_2_flits_send_data),
    .out_w_flits_credit_return(e2w_0_2_flits_credit_return)
  );
  logic tie_l_2_2_flits_send_valid;
  logic [31:0] tie_l_2_2_flits_send_data;
  logic tie_l_2_2_flits_credit_return;
  logic n2s_2_2_flits_send_valid;
  logic [31:0] n2s_2_2_flits_send_data;
  logic n2s_2_2_flits_credit_return;
  logic s2n_2_2_flits_send_valid;
  logic [31:0] s2n_2_2_flits_send_data;
  logic s2n_2_2_flits_credit_return;
  logic e2w_2_2_flits_send_valid;
  logic [31:0] e2w_2_2_flits_send_data;
  logic e2w_2_2_flits_credit_return;
  logic w2e_2_2_flits_send_valid;
  logic [31:0] w2e_2_2_flits_send_data;
  logic w2e_2_2_flits_credit_return;
  Router__X_2_Y_2 #(.X(2), .Y(2)) r_2_2 (
    .clk(clk),
    .rst(rst),
    .in_local_flits_send_valid(tie_l_2_2_flits_send_valid),
    .in_local_flits_send_data(tie_l_2_2_flits_send_data),
    .in_local_flits_credit_return(tie_l_2_2_flits_credit_return),
    .out_local_flits_send_valid(tie_l_2_2_flits_send_valid),
    .out_local_flits_send_data(tie_l_2_2_flits_send_data),
    .out_local_flits_credit_return(tie_l_2_2_flits_credit_return),
    .in_n_flits_send_valid(n2s_2_2_flits_send_valid),
    .in_n_flits_send_data(n2s_2_2_flits_send_data),
    .in_n_flits_credit_return(n2s_2_2_flits_credit_return),
    .out_n_flits_send_valid(s2n_2_2_flits_send_valid),
    .out_n_flits_send_data(s2n_2_2_flits_send_data),
    .out_n_flits_credit_return(s2n_2_2_flits_credit_return),
    .in_s_flits_send_valid(s2n_2_1_flits_send_valid),
    .in_s_flits_send_data(s2n_2_1_flits_send_data),
    .in_s_flits_credit_return(s2n_2_1_flits_credit_return),
    .out_s_flits_send_valid(n2s_2_1_flits_send_valid),
    .out_s_flits_send_data(n2s_2_1_flits_send_data),
    .out_s_flits_credit_return(n2s_2_1_flits_credit_return),
    .in_e_flits_send_valid(e2w_2_2_flits_send_valid),
    .in_e_flits_send_data(e2w_2_2_flits_send_data),
    .in_e_flits_credit_return(e2w_2_2_flits_credit_return),
    .out_e_flits_send_valid(w2e_2_2_flits_send_valid),
    .out_e_flits_send_data(w2e_2_2_flits_send_data),
    .out_e_flits_credit_return(w2e_2_2_flits_credit_return),
    .in_w_flits_send_valid(w2e_1_2_flits_send_valid),
    .in_w_flits_send_data(w2e_1_2_flits_send_data),
    .in_w_flits_credit_return(w2e_1_2_flits_credit_return),
    .out_w_flits_send_valid(e2w_1_2_flits_send_valid),
    .out_w_flits_send_data(e2w_1_2_flits_send_data),
    .out_w_flits_credit_return(e2w_1_2_flits_credit_return)
  );
  logic tie_l_3_2_flits_send_valid;
  logic [31:0] tie_l_3_2_flits_send_data;
  logic tie_l_3_2_flits_credit_return;
  logic n2s_3_2_flits_send_valid;
  logic [31:0] n2s_3_2_flits_send_data;
  logic n2s_3_2_flits_credit_return;
  logic s2n_3_2_flits_send_valid;
  logic [31:0] s2n_3_2_flits_send_data;
  logic s2n_3_2_flits_credit_return;
  logic tie_e_3_2_flits_send_valid;
  logic [31:0] tie_e_3_2_flits_send_data;
  logic tie_e_3_2_flits_credit_return;
  Router__X_3_Y_2 #(.X(3), .Y(2)) r_3_2 (
    .clk(clk),
    .rst(rst),
    .in_local_flits_send_valid(tie_l_3_2_flits_send_valid),
    .in_local_flits_send_data(tie_l_3_2_flits_send_data),
    .in_local_flits_credit_return(tie_l_3_2_flits_credit_return),
    .out_local_flits_send_valid(tie_l_3_2_flits_send_valid),
    .out_local_flits_send_data(tie_l_3_2_flits_send_data),
    .out_local_flits_credit_return(tie_l_3_2_flits_credit_return),
    .in_n_flits_send_valid(n2s_3_2_flits_send_valid),
    .in_n_flits_send_data(n2s_3_2_flits_send_data),
    .in_n_flits_credit_return(n2s_3_2_flits_credit_return),
    .out_n_flits_send_valid(s2n_3_2_flits_send_valid),
    .out_n_flits_send_data(s2n_3_2_flits_send_data),
    .out_n_flits_credit_return(s2n_3_2_flits_credit_return),
    .in_s_flits_send_valid(s2n_3_1_flits_send_valid),
    .in_s_flits_send_data(s2n_3_1_flits_send_data),
    .in_s_flits_credit_return(s2n_3_1_flits_credit_return),
    .out_s_flits_send_valid(n2s_3_1_flits_send_valid),
    .out_s_flits_send_data(n2s_3_1_flits_send_data),
    .out_s_flits_credit_return(n2s_3_1_flits_credit_return),
    .in_e_flits_send_valid(tie_e_3_2_flits_send_valid),
    .in_e_flits_send_data(tie_e_3_2_flits_send_data),
    .in_e_flits_credit_return(tie_e_3_2_flits_credit_return),
    .out_e_flits_send_valid(tie_e_3_2_flits_send_valid),
    .out_e_flits_send_data(tie_e_3_2_flits_send_data),
    .out_e_flits_credit_return(tie_e_3_2_flits_credit_return),
    .in_w_flits_send_valid(w2e_2_2_flits_send_valid),
    .in_w_flits_send_data(w2e_2_2_flits_send_data),
    .in_w_flits_credit_return(w2e_2_2_flits_credit_return),
    .out_w_flits_send_valid(e2w_2_2_flits_send_valid),
    .out_w_flits_send_data(e2w_2_2_flits_send_data),
    .out_w_flits_credit_return(e2w_2_2_flits_credit_return)
  );
  logic tie_l_0_3_flits_send_valid;
  logic [31:0] tie_l_0_3_flits_send_data;
  logic tie_l_0_3_flits_credit_return;
  logic tie_n_0_3_flits_send_valid;
  logic [31:0] tie_n_0_3_flits_send_data;
  logic tie_n_0_3_flits_credit_return;
  logic e2w_0_3_flits_send_valid;
  logic [31:0] e2w_0_3_flits_send_data;
  logic e2w_0_3_flits_credit_return;
  logic w2e_0_3_flits_send_valid;
  logic [31:0] w2e_0_3_flits_send_data;
  logic w2e_0_3_flits_credit_return;
  logic tie_w_0_3_flits_send_valid;
  logic [31:0] tie_w_0_3_flits_send_data;
  logic tie_w_0_3_flits_credit_return;
  Router__X_0_Y_3 #(.X(0), .Y(3)) r_0_3 (
    .clk(clk),
    .rst(rst),
    .in_local_flits_send_valid(tie_l_0_3_flits_send_valid),
    .in_local_flits_send_data(tie_l_0_3_flits_send_data),
    .in_local_flits_credit_return(tie_l_0_3_flits_credit_return),
    .out_local_flits_send_valid(tie_l_0_3_flits_send_valid),
    .out_local_flits_send_data(tie_l_0_3_flits_send_data),
    .out_local_flits_credit_return(tie_l_0_3_flits_credit_return),
    .in_n_flits_send_valid(tie_n_0_3_flits_send_valid),
    .in_n_flits_send_data(tie_n_0_3_flits_send_data),
    .in_n_flits_credit_return(tie_n_0_3_flits_credit_return),
    .out_n_flits_send_valid(tie_n_0_3_flits_send_valid),
    .out_n_flits_send_data(tie_n_0_3_flits_send_data),
    .out_n_flits_credit_return(tie_n_0_3_flits_credit_return),
    .in_s_flits_send_valid(s2n_0_2_flits_send_valid),
    .in_s_flits_send_data(s2n_0_2_flits_send_data),
    .in_s_flits_credit_return(s2n_0_2_flits_credit_return),
    .out_s_flits_send_valid(n2s_0_2_flits_send_valid),
    .out_s_flits_send_data(n2s_0_2_flits_send_data),
    .out_s_flits_credit_return(n2s_0_2_flits_credit_return),
    .in_e_flits_send_valid(e2w_0_3_flits_send_valid),
    .in_e_flits_send_data(e2w_0_3_flits_send_data),
    .in_e_flits_credit_return(e2w_0_3_flits_credit_return),
    .out_e_flits_send_valid(w2e_0_3_flits_send_valid),
    .out_e_flits_send_data(w2e_0_3_flits_send_data),
    .out_e_flits_credit_return(w2e_0_3_flits_credit_return),
    .in_w_flits_send_valid(tie_w_0_3_flits_send_valid),
    .in_w_flits_send_data(tie_w_0_3_flits_send_data),
    .in_w_flits_credit_return(tie_w_0_3_flits_credit_return),
    .out_w_flits_send_valid(tie_w_0_3_flits_send_valid),
    .out_w_flits_send_data(tie_w_0_3_flits_send_data),
    .out_w_flits_credit_return(tie_w_0_3_flits_credit_return)
  );
  logic tie_l_1_3_flits_send_valid;
  logic [31:0] tie_l_1_3_flits_send_data;
  logic tie_l_1_3_flits_credit_return;
  logic tie_n_1_3_flits_send_valid;
  logic [31:0] tie_n_1_3_flits_send_data;
  logic tie_n_1_3_flits_credit_return;
  logic e2w_1_3_flits_send_valid;
  logic [31:0] e2w_1_3_flits_send_data;
  logic e2w_1_3_flits_credit_return;
  logic w2e_1_3_flits_send_valid;
  logic [31:0] w2e_1_3_flits_send_data;
  logic w2e_1_3_flits_credit_return;
  Router__X_1_Y_3 #(.X(1), .Y(3)) r_1_3 (
    .clk(clk),
    .rst(rst),
    .in_local_flits_send_valid(tie_l_1_3_flits_send_valid),
    .in_local_flits_send_data(tie_l_1_3_flits_send_data),
    .in_local_flits_credit_return(tie_l_1_3_flits_credit_return),
    .out_local_flits_send_valid(tie_l_1_3_flits_send_valid),
    .out_local_flits_send_data(tie_l_1_3_flits_send_data),
    .out_local_flits_credit_return(tie_l_1_3_flits_credit_return),
    .in_n_flits_send_valid(tie_n_1_3_flits_send_valid),
    .in_n_flits_send_data(tie_n_1_3_flits_send_data),
    .in_n_flits_credit_return(tie_n_1_3_flits_credit_return),
    .out_n_flits_send_valid(tie_n_1_3_flits_send_valid),
    .out_n_flits_send_data(tie_n_1_3_flits_send_data),
    .out_n_flits_credit_return(tie_n_1_3_flits_credit_return),
    .in_s_flits_send_valid(s2n_1_2_flits_send_valid),
    .in_s_flits_send_data(s2n_1_2_flits_send_data),
    .in_s_flits_credit_return(s2n_1_2_flits_credit_return),
    .out_s_flits_send_valid(n2s_1_2_flits_send_valid),
    .out_s_flits_send_data(n2s_1_2_flits_send_data),
    .out_s_flits_credit_return(n2s_1_2_flits_credit_return),
    .in_e_flits_send_valid(e2w_1_3_flits_send_valid),
    .in_e_flits_send_data(e2w_1_3_flits_send_data),
    .in_e_flits_credit_return(e2w_1_3_flits_credit_return),
    .out_e_flits_send_valid(w2e_1_3_flits_send_valid),
    .out_e_flits_send_data(w2e_1_3_flits_send_data),
    .out_e_flits_credit_return(w2e_1_3_flits_credit_return),
    .in_w_flits_send_valid(w2e_0_3_flits_send_valid),
    .in_w_flits_send_data(w2e_0_3_flits_send_data),
    .in_w_flits_credit_return(w2e_0_3_flits_credit_return),
    .out_w_flits_send_valid(e2w_0_3_flits_send_valid),
    .out_w_flits_send_data(e2w_0_3_flits_send_data),
    .out_w_flits_credit_return(e2w_0_3_flits_credit_return)
  );
  logic tie_l_2_3_flits_send_valid;
  logic [31:0] tie_l_2_3_flits_send_data;
  logic tie_l_2_3_flits_credit_return;
  logic tie_n_2_3_flits_send_valid;
  logic [31:0] tie_n_2_3_flits_send_data;
  logic tie_n_2_3_flits_credit_return;
  logic e2w_2_3_flits_send_valid;
  logic [31:0] e2w_2_3_flits_send_data;
  logic e2w_2_3_flits_credit_return;
  logic w2e_2_3_flits_send_valid;
  logic [31:0] w2e_2_3_flits_send_data;
  logic w2e_2_3_flits_credit_return;
  Router__X_2_Y_3 #(.X(2), .Y(3)) r_2_3 (
    .clk(clk),
    .rst(rst),
    .in_local_flits_send_valid(tie_l_2_3_flits_send_valid),
    .in_local_flits_send_data(tie_l_2_3_flits_send_data),
    .in_local_flits_credit_return(tie_l_2_3_flits_credit_return),
    .out_local_flits_send_valid(tie_l_2_3_flits_send_valid),
    .out_local_flits_send_data(tie_l_2_3_flits_send_data),
    .out_local_flits_credit_return(tie_l_2_3_flits_credit_return),
    .in_n_flits_send_valid(tie_n_2_3_flits_send_valid),
    .in_n_flits_send_data(tie_n_2_3_flits_send_data),
    .in_n_flits_credit_return(tie_n_2_3_flits_credit_return),
    .out_n_flits_send_valid(tie_n_2_3_flits_send_valid),
    .out_n_flits_send_data(tie_n_2_3_flits_send_data),
    .out_n_flits_credit_return(tie_n_2_3_flits_credit_return),
    .in_s_flits_send_valid(s2n_2_2_flits_send_valid),
    .in_s_flits_send_data(s2n_2_2_flits_send_data),
    .in_s_flits_credit_return(s2n_2_2_flits_credit_return),
    .out_s_flits_send_valid(n2s_2_2_flits_send_valid),
    .out_s_flits_send_data(n2s_2_2_flits_send_data),
    .out_s_flits_credit_return(n2s_2_2_flits_credit_return),
    .in_e_flits_send_valid(e2w_2_3_flits_send_valid),
    .in_e_flits_send_data(e2w_2_3_flits_send_data),
    .in_e_flits_credit_return(e2w_2_3_flits_credit_return),
    .out_e_flits_send_valid(w2e_2_3_flits_send_valid),
    .out_e_flits_send_data(w2e_2_3_flits_send_data),
    .out_e_flits_credit_return(w2e_2_3_flits_credit_return),
    .in_w_flits_send_valid(w2e_1_3_flits_send_valid),
    .in_w_flits_send_data(w2e_1_3_flits_send_data),
    .in_w_flits_credit_return(w2e_1_3_flits_credit_return),
    .out_w_flits_send_valid(e2w_1_3_flits_send_valid),
    .out_w_flits_send_data(e2w_1_3_flits_send_data),
    .out_w_flits_credit_return(e2w_1_3_flits_credit_return)
  );
  logic tie_l_3_3_flits_send_valid;
  logic [31:0] tie_l_3_3_flits_send_data;
  logic tie_l_3_3_flits_credit_return;
  logic tie_n_3_3_flits_send_valid;
  logic [31:0] tie_n_3_3_flits_send_data;
  logic tie_n_3_3_flits_credit_return;
  logic tie_e_3_3_flits_send_valid;
  logic [31:0] tie_e_3_3_flits_send_data;
  logic tie_e_3_3_flits_credit_return;
  Router__X_3_Y_3 #(.X(3), .Y(3)) r_3_3 (
    .clk(clk),
    .rst(rst),
    .in_local_flits_send_valid(tie_l_3_3_flits_send_valid),
    .in_local_flits_send_data(tie_l_3_3_flits_send_data),
    .in_local_flits_credit_return(tie_l_3_3_flits_credit_return),
    .out_local_flits_send_valid(snk_link_flits_send_valid),
    .out_local_flits_send_data(snk_link_flits_send_data),
    .out_local_flits_credit_return(snk_link_flits_credit_return),
    .in_n_flits_send_valid(tie_n_3_3_flits_send_valid),
    .in_n_flits_send_data(tie_n_3_3_flits_send_data),
    .in_n_flits_credit_return(tie_n_3_3_flits_credit_return),
    .out_n_flits_send_valid(tie_n_3_3_flits_send_valid),
    .out_n_flits_send_data(tie_n_3_3_flits_send_data),
    .out_n_flits_credit_return(tie_n_3_3_flits_credit_return),
    .in_s_flits_send_valid(s2n_3_2_flits_send_valid),
    .in_s_flits_send_data(s2n_3_2_flits_send_data),
    .in_s_flits_credit_return(s2n_3_2_flits_credit_return),
    .out_s_flits_send_valid(n2s_3_2_flits_send_valid),
    .out_s_flits_send_data(n2s_3_2_flits_send_data),
    .out_s_flits_credit_return(n2s_3_2_flits_credit_return),
    .in_e_flits_send_valid(tie_e_3_3_flits_send_valid),
    .in_e_flits_send_data(tie_e_3_3_flits_send_data),
    .in_e_flits_credit_return(tie_e_3_3_flits_credit_return),
    .out_e_flits_send_valid(tie_e_3_3_flits_send_valid),
    .out_e_flits_send_data(tie_e_3_3_flits_send_data),
    .out_e_flits_credit_return(tie_e_3_3_flits_credit_return),
    .in_w_flits_send_valid(w2e_2_3_flits_send_valid),
    .in_w_flits_send_data(w2e_2_3_flits_send_data),
    .in_w_flits_credit_return(w2e_2_3_flits_credit_return),
    .out_w_flits_send_valid(e2w_2_3_flits_send_valid),
    .out_w_flits_send_data(e2w_2_3_flits_send_data),
    .out_w_flits_credit_return(e2w_2_3_flits_credit_return)
  );

endmodule

