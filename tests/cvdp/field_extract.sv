module field_extract (
  input logic clk,
  input logic rst,
  input logic vld,
  input logic sof,
  input logic eof,
  input logic [31:0] data,
  output logic ack,
  output logic [15:0] field,
  output logic field_vld
);

  typedef enum logic [1:0] {
    IDLE = 2'd0,
    EXTRACTING = 2'd1,
    DONE = 2'd2,
    FAILFINAL = 2'd3
  } field_extract_state_t;
  
  field_extract_state_t state_r, state_next;
  
  logic [3:0] beat_cnt;
  logic [15:0] temp_field;
  logic [15:0] field_r;
  logic field_vld_r;
  
  always_ff @(posedge clk or posedge rst) begin
    if (rst) begin
      state_r <= IDLE;
      beat_cnt <= 0;
      temp_field <= 0;
      field_r <= 0;
      field_vld_r <= 1'b0;
    end else begin
      state_r <= state_next;
      case (state_r)
        IDLE: begin
          beat_cnt <= 0;
        end
        EXTRACTING: begin
          if (vld) begin
            if (beat_cnt == 1) begin
              temp_field <= data[31:16];
            end else begin
              beat_cnt <= 4'(beat_cnt + 1);
            end
          end
        end
        DONE: begin
          field_r <= temp_field;
          if (eof) begin
            field_vld_r <= 1'b0;
          end else begin
            field_vld_r <= 1'b1;
          end
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      IDLE: begin
        if (vld & sof) state_next = EXTRACTING;
      end
      EXTRACTING: begin
        if (vld & beat_cnt == 1) state_next = DONE;
      end
      DONE: begin
        if (eof) state_next = FAILFINAL;
      end
      FAILFINAL: begin
        state_next = IDLE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    ack = 1'b1;
    field = field_r;
    field_vld = field_vld_r;
    case (state_r)
      IDLE: begin
      end
      EXTRACTING: begin
      end
      DONE: begin
      end
      FAILFINAL: begin
      end
      default: ;
    endcase
  end

endmodule

