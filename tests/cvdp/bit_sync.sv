module bit_sync #(
  parameter int STAGES = 2
) (
  input logic aclk,
  input logic bclk,
  input logic rst_n,
  input logic adata,
  output logic aq2_data,
  output logic bq2_data
);

  logic [STAGES-1:0] b_sync_chain;
  logic [STAGES-1:0] a_sync_chain;
  always_ff @(posedge bclk or negedge rst_n) begin
    if ((!rst_n)) begin
      b_sync_chain <= 0;
    end else begin
      b_sync_chain <= {b_sync_chain[STAGES - 2:0], adata};
    end
  end
  assign bq2_data = b_sync_chain[STAGES - 1];
  always_ff @(posedge aclk or negedge rst_n) begin
    if ((!rst_n)) begin
      a_sync_chain <= 0;
    end else begin
      a_sync_chain <= {a_sync_chain[STAGES - 2:0], bq2_data};
    end
  end
  assign aq2_data = a_sync_chain[STAGES - 1];

endmodule

