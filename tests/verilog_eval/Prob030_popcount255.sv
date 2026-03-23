module TopModule (
  input logic [255-1:0] in_sig,
  output logic [8-1:0] out_sig
);

  logic [8-1:0] acc;
  always_comb begin
    acc = 0;
    for (int i = 0; i <= 254; i++) begin
      acc = 8'((acc + 8'($unsigned(in_sig[i]))));
    end
    out_sig = acc;
  end

endmodule

