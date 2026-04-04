module unpack_one_hot (
  input logic sign,
  input logic size,
  input logic [3-1:0] one_hot_selector,
  input logic [256-1:0] source_reg,
  output logic [512-1:0] destination_reg
);

  always_comb begin
    destination_reg = 0;
    if (one_hot_selector == 1) begin
      for (int i = 0; i <= 255; i++) begin
        if (sign & source_reg[i]) begin
          destination_reg[i * 8 +: 8] = 'hFF;
        end else begin
          destination_reg[i * 8 +: 8] = 8'($unsigned(source_reg[i]));
        end
      end
    end else if (one_hot_selector == 2) begin
      for (int i = 0; i <= 127; i++) begin
        if (sign & source_reg[i * 2 + 1]) begin
          destination_reg[i * 8 +: 8] = {source_reg[i * 2 + 1], source_reg[i * 2 + 1], source_reg[i * 2 + 1], source_reg[i * 2 + 1], source_reg[i * 2 + 1], source_reg[i * 2 + 1], source_reg[i * 2 +: 2]};
        end else begin
          destination_reg[i * 8 +: 8] = 8'($unsigned(source_reg[i * 2 +: 2]));
        end
      end
    end else if (one_hot_selector == 4) begin
      if (size) begin
        for (int i = 0; i <= 31; i++) begin
          if (sign & source_reg[i * 8 + 7]) begin
            destination_reg[i * 16 +: 16] = {source_reg[i * 8 + 7], source_reg[i * 8 + 7], source_reg[i * 8 + 7], source_reg[i * 8 + 7], source_reg[i * 8 + 7], source_reg[i * 8 + 7], source_reg[i * 8 + 7], source_reg[i * 8 + 7], source_reg[i * 8 +: 8]};
          end else begin
            destination_reg[i * 16 +: 16] = 16'($unsigned(source_reg[i * 8 +: 8]));
          end
        end
      end else begin
        for (int i = 0; i <= 63; i++) begin
          if (sign & source_reg[i * 4 + 3]) begin
            destination_reg[i * 8 +: 8] = {source_reg[i * 4 + 3], source_reg[i * 4 + 3], source_reg[i * 4 + 3], source_reg[i * 4 + 3], source_reg[i * 4 +: 4]};
          end else begin
            destination_reg[i * 8 +: 8] = 8'($unsigned(source_reg[i * 4 +: 4]));
          end
        end
      end
    end else begin
      destination_reg = 512'($unsigned(source_reg));
    end
  end

endmodule

