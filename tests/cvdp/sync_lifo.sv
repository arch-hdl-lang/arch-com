module sync_lifo #(
  parameter int DATA_WIDTH = 8,
  parameter int ADDR_WIDTH = 3,
  parameter int MEM_DEPTH = 1 << ADDR_WIDTH
) (
  input logic clock,
  input logic reset,
  input logic write_en,
  input logic read_en,
  input logic [DATA_WIDTH-1:0] data_in,
  output logic empty,
  output logic full,
  output logic [DATA_WIDTH-1:0] data_out
);

  logic [ADDR_WIDTH + 1-1:0] sp;
  logic [DATA_WIDTH-1:0] mem [MEM_DEPTH-1:0];
  logic [ADDR_WIDTH + 1-1:0] sp_inc;
  assign sp_inc = (ADDR_WIDTH + 1)'(sp + 1);
  logic [ADDR_WIDTH + 1-1:0] sp_dec;
  assign sp_dec = (ADDR_WIDTH + 1)'(sp - 1);
  logic sp_inc_msb;
  assign sp_inc_msb = sp_inc[ADDR_WIDTH +: 1] == 1;
  always_ff @(posedge clock) begin
    if (reset) begin
      data_out <= 0;
      empty <= 1'b1;
      full <= 1'b0;
      for (int __ri0 = 0; __ri0 < MEM_DEPTH; __ri0++) begin
        mem[__ri0] <= 0;
      end
      sp <= 0;
    end else begin
      if (write_en & ~full) begin
        mem[ADDR_WIDTH'(sp)] <= data_in;
        if (read_en & ~empty) begin
          data_out <= mem[ADDR_WIDTH'(sp_dec)];
        end else begin
          sp <= sp_inc;
          empty <= 1'b0;
          if (sp_inc_msb) begin
            full <= 1'b1;
          end
        end
      end else if (read_en & ~empty) begin
        sp <= sp_dec;
        data_out <= mem[ADDR_WIDTH'(sp_dec)];
        full <= 1'b0;
        if (sp_dec == 0) begin
          empty <= 1'b1;
        end
      end
    end
  end

endmodule

