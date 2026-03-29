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

module mux_synch (
  input logic [8-1:0] data_in,
  input logic req,
  input logic dst_clk,
  input logic src_clk,
  input logic nrst,
  output logic [8-1:0] data_out
);

  // Synchronize req to dst_clk domain using 2-flop synchronizer
  logic req_syncd;
  nff sync_req (
    .d_in(req),
    .dst_clk(dst_clk),
    .rst(nrst),
    .syncd(req_syncd)
  );
  // Mux: select data_in when req_syncd is high, else feed back data_out
  logic [8-1:0] mux_out;
  always_comb begin
    if (req_syncd) begin
      mux_out = data_in;
    end else begin
      mux_out = data_out;
    end
  end
  always_ff @(posedge dst_clk or negedge nrst) begin
    if ((!nrst)) begin
      data_out <= 0;
    end else begin
      data_out <= mux_out;
    end
  end

endmodule

