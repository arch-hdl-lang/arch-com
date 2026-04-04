module data_bus_controller #(
  parameter int AFINITY = 0
) (
  input logic clk,
  input logic rst_n,
  output logic m0_ready,
  input logic m0_valid,
  input logic [32-1:0] m0_data,
  output logic m1_ready,
  input logic m1_valid,
  input logic [32-1:0] m1_data,
  input logic s_ready,
  output logic s_valid,
  output logic [32-1:0] s_data
);

  assign m0_ready = s_ready;
  assign m1_ready = s_ready;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      s_data <= 0;
      s_valid <= 1'b0;
    end else begin
      if (m0_valid & m1_valid) begin
        if (AFINITY == 0) begin
          s_valid <= 1'b1;
          s_data <= m0_data;
        end else begin
          s_valid <= 1'b1;
          s_data <= m1_data;
        end
      end else if (m0_valid) begin
        s_valid <= 1'b1;
        s_data <= m0_data;
      end else if (m1_valid) begin
        s_valid <= 1'b1;
        s_data <= m1_data;
      end else begin
        s_valid <= 1'b0;
      end
    end
  end

endmodule

