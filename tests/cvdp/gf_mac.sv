module gf_mac #(
  parameter int WIDTH = 8,
  localparam int SEGMENTS = WIDTH / 8,
  localparam int WIDTH_VALID = WIDTH > 0 && WIDTH % 8 == 0
) (
  input logic [WIDTH-1:0] a,
  input logic [WIDTH-1:0] b,
  output logic [8-1:0] result,
  output logic error_flag,
  output logic valid_result
);

  function automatic logic [8-1:0] xtime(input logic [8-1:0] a);
    logic [8-1:0] shifted = a << 1;
    return a[7:7] == 1 ? shifted ^ 8'd27 : shifted;
  endfunction
  
  function automatic logic [8-1:0] gf_mul8(input logic [8-1:0] x, input logic [8-1:0] y);
    logic [8-1:0] mut_a0 = x;
    logic [8-1:0] mut_a1 = xtime(mut_a0);
    logic [8-1:0] mut_a2 = xtime(mut_a1);
    logic [8-1:0] mut_a3 = xtime(mut_a2);
    logic [8-1:0] mut_a4 = xtime(mut_a3);
    logic [8-1:0] mut_a5 = xtime(mut_a4);
    logic [8-1:0] mut_a6 = xtime(mut_a5);
    logic [8-1:0] mut_a7 = xtime(mut_a6);
    logic [8-1:0] part0 = y[0:0] == 1 ? mut_a0 : 0;
    logic [8-1:0] part1 = y[1:1] == 1 ? mut_a1 : 0;
    logic [8-1:0] part2 = y[2:2] == 1 ? mut_a2 : 0;
    logic [8-1:0] part3 = y[3:3] == 1 ? mut_a3 : 0;
    logic [8-1:0] part4 = y[4:4] == 1 ? mut_a4 : 0;
    logic [8-1:0] part5 = y[5:5] == 1 ? mut_a5 : 0;
    logic [8-1:0] part6 = y[6:6] == 1 ? mut_a6 : 0;
    logic [8-1:0] part7 = y[7:7] == 1 ? mut_a7 : 0;
    return part0 ^ part1 ^ part2 ^ part3 ^ part4 ^ part5 ^ part6 ^ part7;
  endfunction
  
  always_comb begin
    result = 0;
    error_flag = ~WIDTH_VALID;
    valid_result = WIDTH_VALID;
    if (WIDTH_VALID) begin
      for (int i = 0; i <= SEGMENTS - 1; i++) begin
        result = result ^ gf_mul8(a[i * 8 +: 8], b[i * 8 +: 8]);
      end
    end
  end

endmodule

