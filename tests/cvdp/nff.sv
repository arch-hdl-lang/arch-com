module nff (
  input logic d_in,
  input logic dst_clk,
  input logic rst,
  output logic syncd
);

  logic ff1;
  always_ff @(posedge dst_clk or negedge rst) begin
    if ((!rst)) begin
      ff1 <= 0;
    end else begin
      ff1 <= d_in;
    end
  end
  always_ff @(posedge dst_clk or negedge rst) begin
    if ((!rst)) begin
      syncd <= 0;
    end else begin
      syncd <= ff1;
    end
  end

endmodule

