module one_hot_gen #(
  parameter int NS_A = 8,
  parameter int NS_B = 4
) (
  input logic clk,
  input logic rst_async_n,
  input logic [1:0] i_config,
  input logic i_start,
  output logic o_ready,
  output logic [NS_A + NS_B-1:0] o_address_one_hot
);

  // 0=IDLE, 1=REGION_A, 2=REGION_B
  logic [1:0] state_ff;
  logic [$clog2(NS_A + NS_B + 1)-1:0] position;
  logic [1:0] active_config;
  logic [31:0] pos32;
  assign pos32 = 32'($unsigned(position));
  always_ff @(posedge clk or negedge rst_async_n) begin
    if ((!rst_async_n)) begin
      active_config <= 0;
      o_address_one_hot <= 0;
      o_ready <= 1'b1;
      position <= 1;
      state_ff <= 0;
    end else begin
      if (state_ff == 0) begin
        if (i_start) begin
          if ((i_config & 1) == 0) begin
            state_ff <= 1;
            active_config <= i_config;
            o_address_one_hot <= (NS_A + NS_B)'($unsigned(1)) << (NS_A + NS_B) - 1;
            position <= ($clog2(NS_A + NS_B + 1))'($unsigned(2));
            o_ready <= 1'b0;
          end else begin
            state_ff <= 2;
            active_config <= i_config;
            o_address_one_hot <= (NS_A + NS_B)'($unsigned(1)) << NS_B - 1;
            position <= ($clog2(NS_A + NS_B + 1))'($unsigned(2));
            o_ready <= 1'b0;
          end
        end else begin
          o_address_one_hot <= 0;
          position <= ($clog2(NS_A + NS_B + 1))'($unsigned(1));
          o_ready <= 1'b1;
        end
      end else if (state_ff == 1) begin
        if (pos32 < NS_A + 1) begin
          o_address_one_hot <= (NS_A + NS_B)'($unsigned(1)) << (NS_A + NS_B) - pos32;
          position <= ($clog2(NS_A + NS_B + 1))'(position + 1);
          o_ready <= 1'b0;
        end else if (active_config == 2) begin
          state_ff <= 2;
          o_address_one_hot <= (NS_A + NS_B)'($unsigned(1)) << NS_B - 1;
          position <= ($clog2(NS_A + NS_B + 1))'($unsigned(2));
          o_ready <= 1'b0;
        end else begin
          state_ff <= 0;
          position <= ($clog2(NS_A + NS_B + 1))'($unsigned(1));
          o_address_one_hot <= 0;
          o_ready <= 1'b1;
        end
      end else if (pos32 < NS_B + 1) begin
        o_address_one_hot <= (NS_A + NS_B)'($unsigned(1)) << NS_B - pos32;
        position <= ($clog2(NS_A + NS_B + 1))'(position + 1);
        o_ready <= 1'b0;
      end else if (active_config == 3) begin
        state_ff <= 1;
        o_address_one_hot <= (NS_A + NS_B)'($unsigned(1)) << (NS_A + NS_B) - 1;
        position <= ($clog2(NS_A + NS_B + 1))'($unsigned(2));
        o_ready <= 1'b0;
      end else begin
        state_ff <= 0;
        position <= ($clog2(NS_A + NS_B + 1))'($unsigned(1));
        o_address_one_hot <= 0;
        o_ready <= 1'b1;
      end
    end
  end

endmodule

