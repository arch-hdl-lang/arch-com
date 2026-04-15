package VgaPkg;
  typedef enum logic [1:0] {
    ST_ACTIVE = 2'd0,
    ST_FRONT = 2'd1,
    ST_PULSE = 2'd2,
    ST_BACK = 2'd3
  } VgaPhase;
  
endpackage

import VgaPkg::*;
module vga_controller #(
  parameter [9:0] H_ACTIVE = 640,
  parameter [9:0] H_FRONT = 16,
  parameter [9:0] H_PULSE = 96,
  parameter [9:0] H_BACK = 48,
  parameter [9:0] V_ACTIVE = 480,
  parameter [9:0] V_FRONT = 10,
  parameter [9:0] V_PULSE = 2,
  parameter [9:0] V_BACK = 33
) (
  input logic clock,
  input logic reset,
  input logic [7:0] color_in,
  output logic hsync,
  output logic vsync,
  output logic [7:0] red,
  output logic [7:0] green,
  output logic [7:0] blue,
  output logic [9:0] next_x,
  output logic [9:0] next_y,
  output logic sync,
  output logic clk_out,
  output logic blank
);

  logic [9:0] h_counter;
  logic [9:0] v_counter;
  VgaPhase h_state;
  VgaPhase v_state;
  logic line_done;
  logic h_active;
  logic v_active;
  assign sync = 1'b0;
  assign clk_out = clock;
  assign h_active = h_state == ST_ACTIVE;
  assign v_active = v_state == ST_ACTIVE;
  always_ff @(posedge clock or posedge reset) begin
    if (reset) begin
      h_counter <= 0;
      h_state <= ST_ACTIVE;
      line_done <= 1'b0;
    end else begin
      if (h_state == ST_ACTIVE) begin
        if (h_counter == H_ACTIVE - 1) begin
          h_counter <= 0;
          h_state <= ST_FRONT;
        end else begin
          h_counter <= 10'(h_counter + 1);
        end
      end else if (h_state == ST_FRONT) begin
        if (h_counter == H_FRONT - 1) begin
          h_counter <= 0;
          h_state <= ST_PULSE;
        end else begin
          h_counter <= 10'(h_counter + 1);
        end
      end else if (h_state == ST_PULSE) begin
        if (h_counter == H_PULSE - 1) begin
          h_counter <= 0;
          h_state <= ST_BACK;
        end else begin
          h_counter <= 10'(h_counter + 1);
        end
      end else if (h_counter == H_BACK - 1) begin
        h_counter <= 0;
        h_state <= ST_ACTIVE;
        line_done <= 1'b1;
      end else begin
        h_counter <= 10'(h_counter + 1);
        line_done <= 1'b0;
      end
    end
  end
  always_ff @(posedge clock or posedge reset) begin
    if (reset) begin
      v_counter <= 0;
      v_state <= ST_ACTIVE;
    end else begin
      if (line_done) begin
        if (v_state == ST_ACTIVE) begin
          if (v_counter == V_ACTIVE - 1) begin
            v_counter <= 0;
            v_state <= ST_FRONT;
          end else begin
            v_counter <= 10'(v_counter + 1);
          end
        end else if (v_state == ST_FRONT) begin
          if (v_counter == V_FRONT - 1) begin
            v_counter <= 0;
            v_state <= ST_PULSE;
          end else begin
            v_counter <= 10'(v_counter + 1);
          end
        end else if (v_state == ST_PULSE) begin
          if (v_counter == V_PULSE - 1) begin
            v_counter <= 0;
            v_state <= ST_BACK;
          end else begin
            v_counter <= 10'(v_counter + 1);
          end
        end else if (v_counter == V_BACK - 1) begin
          v_counter <= 0;
          v_state <= ST_ACTIVE;
        end else begin
          v_counter <= 10'(v_counter + 1);
        end
      end
    end
  end
  always_ff @(posedge clock or posedge reset) begin
    if (reset) begin
      hsync <= 1'b1;
      vsync <= 1'b1;
    end else begin
      if (h_state == ST_PULSE) begin
        hsync <= 1'b0;
      end else begin
        hsync <= 1'b1;
      end
      if (v_state == ST_PULSE) begin
        vsync <= 1'b0;
      end else begin
        vsync <= 1'b1;
      end
    end
  end
  always_ff @(posedge clock or posedge reset) begin
    if (reset) begin
      blank <= 0;
      blue <= 0;
      green <= 0;
      next_x <= 0;
      next_y <= 0;
      red <= 0;
    end else begin
      if (h_active) begin
        if (v_active) begin
          red <= {color_in[7:5], 5'd0};
          green <= {color_in[4:2], 5'd0};
          blue <= {color_in[1:0], 6'd0};
          next_x <= h_counter;
          next_y <= v_counter;
          blank <= 1'b0;
        end else begin
          red <= 0;
          green <= 0;
          blue <= 0;
          next_x <= 0;
          next_y <= 0;
          blank <= 1'b1;
        end
      end else begin
        red <= 0;
        green <= 0;
        blue <= 0;
        next_x <= 0;
        next_y <= 0;
        blank <= 1'b1;
      end
    end
  end

endmodule

