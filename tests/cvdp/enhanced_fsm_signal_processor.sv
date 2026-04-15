module enhanced_fsm_signal_processor (
  input logic i_clk,
  input logic i_rst_n,
  input logic i_enable,
  input logic i_clear,
  input logic i_ack,
  input logic i_fault,
  input logic [4:0] i_vector_1,
  input logic [4:0] i_vector_2,
  input logic [4:0] i_vector_3,
  input logic [4:0] i_vector_4,
  input logic [4:0] i_vector_5,
  input logic [4:0] i_vector_6,
  output logic o_ready,
  output logic o_error,
  output logic [1:0] o_fsm_status,
  output logic [7:0] o_vector_1,
  output logic [7:0] o_vector_2,
  output logic [7:0] o_vector_3,
  output logic [7:0] o_vector_4
);

  // 1-cycle "just entered" flag: suppress action on the cycle a state is entered
  logic entered;
  logic [1:0] two_ones;
  assign two_ones = 3;
  logic [31:0] concat_bus;
  assign concat_bus = {i_vector_1, i_vector_2, i_vector_3, i_vector_4, i_vector_5, i_vector_6, two_ones};
  always_ff @(posedge i_clk) begin
    if ((!i_rst_n)) begin
      entered <= 0;
      o_error <= 0;
      o_fsm_status <= 0;
      o_ready <= 0;
      o_vector_1 <= 0;
      o_vector_2 <= 0;
      o_vector_3 <= 0;
      o_vector_4 <= 0;
    end else begin
      if (entered) begin
        entered <= 0;
      end else if (o_fsm_status == 2'd0) begin
        // IDLE
        if (i_fault) begin
          o_fsm_status <= 2'd3;
          o_error <= 1;
          o_ready <= 0;
          o_vector_1 <= 0;
          o_vector_2 <= 0;
          o_vector_3 <= 0;
          o_vector_4 <= 0;
          entered <= 1;
        end else if (i_enable) begin
          o_fsm_status <= 2'd1;
          entered <= 1;
        end
      end else if (o_fsm_status == 2'd1) begin
        // PROCESS
        if (i_fault) begin
          o_fsm_status <= 2'd3;
          o_error <= 1;
          o_ready <= 0;
          o_vector_1 <= 0;
          o_vector_2 <= 0;
          o_vector_3 <= 0;
          o_vector_4 <= 0;
          entered <= 1;
        end else begin
          o_vector_1 <= concat_bus[31:24];
          o_vector_2 <= concat_bus[23:16];
          o_vector_3 <= concat_bus[15:8];
          o_vector_4 <= concat_bus[7:0];
          o_fsm_status <= 2'd2;
          o_ready <= 1;
          entered <= 1;
        end
      end else if (o_fsm_status == 2'd2) begin
        // READY
        if (i_fault) begin
          o_fsm_status <= 2'd3;
          o_error <= 1;
          o_ready <= 0;
          o_vector_1 <= 0;
          o_vector_2 <= 0;
          o_vector_3 <= 0;
          o_vector_4 <= 0;
          entered <= 1;
        end else if (i_ack) begin
          o_fsm_status <= 2'd0;
          o_ready <= 0;
          entered <= 1;
        end
      end else if (o_fsm_status == 2'd3) begin
        // FAULT
        if (~i_fault & i_clear) begin
          o_fsm_status <= 2'd0;
          o_error <= 0;
          o_ready <= 0;
          o_vector_1 <= 0;
          o_vector_2 <= 0;
          o_vector_3 <= 0;
          o_vector_4 <= 0;
          entered <= 1;
        end
      end
    end
  end

endmodule

