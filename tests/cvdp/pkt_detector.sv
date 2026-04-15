module pkt_detector #(
  parameter int PKT_CNT_WIDTH = 4
) (
  input logic clk,
  input logic reset,
  input logic [7:0] data_in,
  input logic data_k_flag,
  output logic [PKT_CNT_WIDTH-1:0] pkt_count,
  output logic [159:0] pkt_data,
  output logic mem_read_detected,
  output logic mem_write_detected,
  output logic io_read_detected,
  output logic io_write_detected,
  output logic cfg_read0_detected,
  output logic cfg_write0_detected,
  output logic cfg_read1_detected,
  output logic cfg_write1_detected,
  output logic completion_detected,
  output logic completion_data_detected,
  output logic error_detected
);

  logic [7:0] START_SYMBOL;
  assign START_SYMBOL = 8'd251;
  logic [7:0] END_SYMBOL;
  assign END_SYMBOL = 8'd253;
  logic [7:0] PKT_BYTES;
  assign PKT_BYTES = 20;
  logic [1:0] curr_state;
  logic [1:0] nxt_state;
  logic [7:0] byte_cnt;
  logic [159:0] pkt_reg;
  logic [PKT_CNT_WIDTH-1:0] pkt_count_r;
  logic [159:0] pkt_data_r;
  logic mem_read_detected_r;
  logic mem_write_detected_r;
  logic io_read_detected_r;
  logic io_write_detected_r;
  logic cfg_read0_detected_r;
  logic cfg_write0_detected_r;
  logic cfg_read1_detected_r;
  logic cfg_write1_detected_r;
  logic completion_detected_r;
  logic completion_data_detected_r;
  logic error_detected_r;
  logic data_is_start;
  assign data_is_start = data_in == START_SYMBOL;
  logic is_start;
  assign is_start = data_is_start & data_k_flag;
  logic [7:0] header;
  assign header = pkt_reg[31:24];
  logic [7:0] end_byte;
  assign end_byte = pkt_reg[159:152];
  logic end_valid;
  assign end_valid = end_byte == END_SYMBOL;
  // Next state logic
  always_comb begin
    if (curr_state == 0) begin
      if (is_start) begin
        nxt_state = 1;
      end else begin
        nxt_state = 0;
      end
    end else if (curr_state == 1) begin
      if (byte_cnt == PKT_BYTES) begin
        nxt_state = 2;
      end else begin
        nxt_state = 1;
      end
    end else if (curr_state == 2) begin
      if (end_valid) begin
        nxt_state = 0;
      end else begin
        nxt_state = 3;
      end
    end else if (is_start) begin
      nxt_state = 1;
    end else begin
      nxt_state = 3;
    end
  end
  // State register
  always_ff @(posedge clk or negedge reset) begin
    if ((!reset)) begin
      curr_state <= 0;
    end else begin
      curr_state <= nxt_state;
    end
  end
  // Datapath
  always_ff @(posedge clk or negedge reset) begin
    if ((!reset)) begin
      byte_cnt <= 0;
      cfg_read0_detected_r <= 1'b0;
      cfg_read1_detected_r <= 1'b0;
      cfg_write0_detected_r <= 1'b0;
      cfg_write1_detected_r <= 1'b0;
      completion_data_detected_r <= 1'b0;
      completion_detected_r <= 1'b0;
      error_detected_r <= 1'b0;
      io_read_detected_r <= 1'b0;
      io_write_detected_r <= 1'b0;
      mem_read_detected_r <= 1'b0;
      mem_write_detected_r <= 1'b0;
      pkt_count_r <= 0;
      pkt_data_r <= 0;
      pkt_reg <= 0;
    end else begin
      if (curr_state == 0) begin
        byte_cnt <= 0;
        pkt_reg <= 0;
        error_detected_r <= 1'b0;
        mem_read_detected_r <= 1'b0;
        mem_write_detected_r <= 1'b0;
        io_read_detected_r <= 1'b0;
        io_write_detected_r <= 1'b0;
        cfg_read0_detected_r <= 1'b0;
        cfg_write0_detected_r <= 1'b0;
        cfg_read1_detected_r <= 1'b0;
        cfg_write1_detected_r <= 1'b0;
        completion_detected_r <= 1'b0;
        completion_data_detected_r <= 1'b0;
        if (is_start) begin
          pkt_reg <= {data_in, 152'd0};
          byte_cnt <= 1;
        end
      end else if (curr_state == 1) begin
        if (byte_cnt < PKT_BYTES) begin
          pkt_reg <= {data_in, pkt_reg[159:8]};
          byte_cnt <= 8'(byte_cnt + 1);
        end
      end else if (curr_state == 2) begin
        if (end_valid) begin
          pkt_data_r <= pkt_reg;
          pkt_count_r <= PKT_CNT_WIDTH'(pkt_count_r + 1);
          if (header == 8'd0) begin
            mem_read_detected_r <= 1'b1;
          end else if (header == 8'd1) begin
            mem_write_detected_r <= 1'b1;
          end else if (header == 8'd2) begin
            io_read_detected_r <= 1'b1;
          end else if (header == 8'd66) begin
            io_write_detected_r <= 1'b1;
          end else if (header == 8'd4) begin
            cfg_read0_detected_r <= 1'b1;
          end else if (header == 8'd68) begin
            cfg_write0_detected_r <= 1'b1;
          end else if (header == 8'd5) begin
            cfg_read1_detected_r <= 1'b1;
          end else if (header == 8'd69) begin
            cfg_write1_detected_r <= 1'b1;
          end else if (header == 8'd10) begin
            completion_detected_r <= 1'b1;
          end else if (header == 8'd74) begin
            completion_data_detected_r <= 1'b1;
          end
        end else begin
          error_detected_r <= 1'b1;
        end
      end else begin
        error_detected_r <= 1'b1;
        byte_cnt <= 0;
        pkt_reg <= 0;
        if (is_start) begin
          pkt_reg <= {data_in, 152'd0};
          byte_cnt <= 1;
          error_detected_r <= 1'b0;
        end
      end
    end
  end
  // Output assignments
  assign pkt_count = pkt_count_r;
  assign pkt_data = pkt_data_r;
  assign mem_read_detected = mem_read_detected_r;
  assign mem_write_detected = mem_write_detected_r;
  assign io_read_detected = io_read_detected_r;
  assign io_write_detected = io_write_detected_r;
  assign cfg_read0_detected = cfg_read0_detected_r;
  assign cfg_write0_detected = cfg_write0_detected_r;
  assign cfg_read1_detected = cfg_read1_detected_r;
  assign cfg_write1_detected = cfg_write1_detected_r;
  assign completion_detected = completion_detected_r;
  assign completion_data_detected = completion_data_detected_r;
  assign error_detected = error_detected_r;

endmodule

