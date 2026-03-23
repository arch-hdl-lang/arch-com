// VerilogEval Prob144: 16x16 toroidal Game of Life
// domain SysDomain

module TopModule (
  input logic clk,
  input logic load,
  input logic [256-1:0] data,
  output logic [256-1:0] q
);

  logic [256-1:0] grid = 0;
  logic [256-1:0] next_grid;
  logic [4-1:0] row;
  logic [4-1:0] col;
  logic [4-1:0] ra;
  logic [4-1:0] rb;
  logic [4-1:0] cl;
  logic [4-1:0] cr;
  logic [4-1:0] ncount;
  always_comb begin
    row = 0;
    col = 0;
    ra = 0;
    rb = 0;
    cl = 0;
    cr = 0;
    ncount = 0;
    for (int i = 0; i <= 255; i++) begin
      row = 4'((i / 16));
      col = 4'((i % 16));
      ra = 4'((row + 15));
      rb = 4'((row + 1));
      cl = 4'((col + 15));
      cr = 4'((col + 1));
      ncount = 4'((((((((4'($unsigned(grid[((ra * 16) + cl)])) + 4'($unsigned(grid[((ra * 16) + col)]))) + 4'($unsigned(grid[((ra * 16) + cr)]))) + 4'($unsigned(grid[((row * 16) + cl)]))) + 4'($unsigned(grid[((row * 16) + cr)]))) + 4'($unsigned(grid[((rb * 16) + cl)]))) + 4'($unsigned(grid[((rb * 16) + col)]))) + 4'($unsigned(grid[((rb * 16) + cr)]))));
      next_grid[i] = ((ncount == 3) | ((ncount == 2) & grid[i]));
    end
  end
  // alive = (ncount==3) | (ncount==2 & grid[i])
  always_ff @(posedge clk) begin
    if (load) begin
      grid <= data;
    end else begin
      grid <= next_grid;
    end
  end
  assign q = grid;

endmodule

