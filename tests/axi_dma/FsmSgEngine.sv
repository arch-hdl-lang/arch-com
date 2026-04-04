// Scatter-Gather descriptor engine.
// Fetches 4-word descriptors from memory via AXI4, triggers data transfers,
// writes completion status back, then chains to the next descriptor.
//
// Descriptor format (16 bytes, 4 words):
//   Word 0: NXTDESC    — next descriptor pointer [31:0]
//   Word 1: BUF_ADDR   — buffer address [31:0]
//   Word 2: CONTROL    — length in bytes [25:0]
//   Word 3: STATUS     — DMA writes: transferred[25:0] | Cmplt[31]
module FsmSgEngine (
  input logic clk,
  input logic rst,
  input logic sg_start,
  input logic [32-1:0] curdesc,
  input logic [32-1:0] taildesc,
  output logic xfer_start,
  output logic [32-1:0] xfer_addr,
  output logic [8-1:0] xfer_num_beats,
  input logic xfer_done,
  output logic sg_done,
  output logic sg_axi_ar_valid,
  input logic sg_axi_ar_ready,
  output logic [32-1:0] sg_axi_ar_addr,
  output logic [1-1:0] sg_axi_ar_id,
  output logic [8-1:0] sg_axi_ar_len,
  output logic [3-1:0] sg_axi_ar_size,
  output logic [2-1:0] sg_axi_ar_burst,
  input logic sg_axi_r_valid,
  output logic sg_axi_r_ready,
  input logic [32-1:0] sg_axi_r_data,
  input logic [1-1:0] sg_axi_r_id,
  input logic [2-1:0] sg_axi_r_resp,
  input logic sg_axi_r_last,
  output logic sg_axi_aw_valid,
  input logic sg_axi_aw_ready,
  output logic [32-1:0] sg_axi_aw_addr,
  output logic [1-1:0] sg_axi_aw_id,
  output logic [8-1:0] sg_axi_aw_len,
  output logic [3-1:0] sg_axi_aw_size,
  output logic [2-1:0] sg_axi_aw_burst,
  output logic sg_axi_w_valid,
  input logic sg_axi_w_ready,
  output logic [32-1:0] sg_axi_w_data,
  output logic [4-1:0] sg_axi_w_strb,
  output logic sg_axi_w_last,
  input logic sg_axi_b_valid,
  output logic sg_axi_b_ready,
  input logic [1-1:0] sg_axi_b_id,
  input logic [2-1:0] sg_axi_b_resp
);

  typedef enum logic [3:0] {
    IDLE = 4'd0,
    FETCHAR = 4'd1,
    FETCHR = 4'd2,
    RUNXFER = 4'd3,
    STATUSAW = 4'd4,
    STATUSW = 4'd5,
    STATUSB = 4'd6,
    CHECKNEXT = 4'd7,
    DONE = 4'd8
  } FsmSgEngine_state_t;
  
  FsmSgEngine_state_t state_r, state_next;
  
  logic [32-1:0] curdesc_r;
  logic [32-1:0] taildesc_r;
  logic [32-1:0] nxtdesc_r;
  logic [32-1:0] buf_addr_r;
  logic [32-1:0] xfer_len_r;
  logic [2-1:0] fetch_idx;
  
  always_ff @(posedge clk) begin
    if (rst) begin
      state_r <= IDLE;
      fetch_idx <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        IDLE: begin
          // Control from register block
          // Transfer interface — drives the data-path FSM (FsmMm2s or FsmS2mm)
          // Completion
          // AXI4 full master — descriptor fetch (read) + status writeback (write)
          // Internal registers
          // ── Idle: wait for sg_start ──────────────────────────────────────────
          if (sg_start) begin
            curdesc_r <= curdesc;
            taildesc_r <= taildesc;
          end
        end
        FETCHAR: begin
          // ── FetchAR: issue AXI4 read for 4-word descriptor ──────────────────
          fetch_idx <= 0;
        end
        FETCHR: begin
          // ── FetchR: receive 4 descriptor words ──────────────────────────────
          if (sg_axi_r_valid) begin
            if (fetch_idx == 0) begin
              nxtdesc_r <= sg_axi_r_data;
            end else if (fetch_idx == 1) begin
              buf_addr_r <= sg_axi_r_data;
            end else if (fetch_idx == 2) begin
              xfer_len_r <= sg_axi_r_data;
            end
            fetch_idx <= 2'(fetch_idx + 1);
          end
        end
        CHECKNEXT: begin
          // ── RunXfer: trigger data transfer and wait ─────────────────────────
          // ── StatusAW: write completion status back to descriptor word 3 ─────
          // ── StatusW: send status word (Cmplt[31] | length[25:0]) ───────────
          // ── StatusB: wait for write response ────────────────────────────────
          // ── CheckNext: advance to next descriptor or finish ─────────────────
          curdesc_r <= nxtdesc_r;
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      IDLE: begin
        if (sg_start) state_next = FETCHAR;
      end
      FETCHAR: begin
        if (sg_axi_ar_ready) state_next = FETCHR;
      end
      FETCHR: begin
        if (sg_axi_r_valid & sg_axi_r_last) state_next = RUNXFER;
      end
      RUNXFER: begin
        if (xfer_done) state_next = STATUSAW;
      end
      STATUSAW: begin
        if (sg_axi_aw_ready) state_next = STATUSW;
      end
      STATUSW: begin
        if (sg_axi_w_ready) state_next = STATUSB;
      end
      STATUSB: begin
        if (sg_axi_b_valid) state_next = CHECKNEXT;
      end
      CHECKNEXT: begin
        if (curdesc_r == taildesc_r) state_next = DONE;
        else if (curdesc_r != taildesc_r) state_next = FETCHAR;
      end
      DONE: begin
        state_next = IDLE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    xfer_start = 1'b0;
    xfer_addr = 0;
    xfer_num_beats = 0;
    sg_done = 1'b0;
    sg_axi_ar_valid = 1'b0;
    sg_axi_ar_addr = 0;
    sg_axi_ar_id = 0;
    sg_axi_ar_len = 0;
    sg_axi_ar_size = 0;
    sg_axi_ar_burst = 0;
    sg_axi_r_ready = 1'b0;
    sg_axi_aw_valid = 1'b0;
    sg_axi_aw_addr = 0;
    sg_axi_aw_id = 0;
    sg_axi_aw_len = 0;
    sg_axi_aw_size = 0;
    sg_axi_aw_burst = 0;
    sg_axi_w_valid = 1'b0;
    sg_axi_w_data = 0;
    sg_axi_w_strb = 0;
    sg_axi_w_last = 1'b0;
    sg_axi_b_ready = 1'b0;
    case (state_r)
      IDLE: begin
      end
      FETCHAR: begin
        sg_axi_ar_valid = 1'b1;
        sg_axi_ar_addr = curdesc_r;
        sg_axi_ar_len = 3;
        sg_axi_ar_size = 2;
        sg_axi_ar_burst = 1;
      end
      FETCHR: begin
        sg_axi_r_ready = 1'b1;
      end
      RUNXFER: begin
        xfer_start = 1'b1;
        xfer_addr = buf_addr_r;
        xfer_num_beats = 8'(xfer_len_r[25:2]);
      end
      STATUSAW: begin
        sg_axi_aw_valid = 1'b1;
        sg_axi_aw_addr = 32'(curdesc_r + 12);
        sg_axi_aw_len = 0;
        sg_axi_aw_size = 2;
        sg_axi_aw_burst = 1;
      end
      STATUSW: begin
        sg_axi_w_valid = 1'b1;
        sg_axi_w_data = {1'd1, 5'd0, xfer_len_r[25:0]};
        sg_axi_w_strb = 'hF;
        sg_axi_w_last = 1'b1;
      end
      STATUSB: begin
        sg_axi_b_ready = 1'b1;
      end
      CHECKNEXT: begin
      end
      DONE: begin
        // ── Done ────────────────────────────────────────────────────────────
        sg_done = 1'b1;
      end
      default: ;
    endcase
  end

endmodule

