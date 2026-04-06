// E203 SRAM Container
// Instantiates ITCM RAM and DTCM RAM, forwarding all ports.
module e203_srams (
  input logic test_mode,
  input logic clk_itcm_ram,
  input logic rst_itcm,
  input logic itcm_ram_sd,
  input logic itcm_ram_ds,
  input logic itcm_ram_ls,
  input logic itcm_ram_cs,
  input logic itcm_ram_we,
  input logic [13-1:0] itcm_ram_addr,
  input logic [8-1:0] itcm_ram_wem,
  input logic [64-1:0] itcm_ram_din,
  output logic [64-1:0] itcm_ram_dout,
  input logic clk_dtcm_ram,
  input logic rst_dtcm,
  input logic dtcm_ram_sd,
  input logic dtcm_ram_ds,
  input logic dtcm_ram_ls,
  input logic dtcm_ram_cs,
  input logic dtcm_ram_we,
  input logic [14-1:0] dtcm_ram_addr,
  input logic [4-1:0] dtcm_ram_wem,
  input logic [32-1:0] dtcm_ram_din,
  output logic [32-1:0] dtcm_ram_dout
);

  // ITCM RAM interface
  // DTCM RAM interface
  // ITCM RAM instance
  logic [64-1:0] itcm_ram_dout_w;
  e203_itcm_ram u_itcm_ram (
    .clk(clk_itcm_ram),
    .rst_n(rst_itcm),
    .sd(itcm_ram_sd),
    .ds(itcm_ram_ds),
    .ls(itcm_ram_ls),
    .cs(itcm_ram_cs),
    .we(itcm_ram_we),
    .addr(itcm_ram_addr),
    .wem(itcm_ram_wem),
    .din(itcm_ram_din),
    .dout(itcm_ram_dout_w)
  );
  // DTCM RAM instance
  logic [32-1:0] dtcm_ram_dout_w;
  e203_dtcm_ram u_dtcm_ram (
    .clk(clk_dtcm_ram),
    .rst_n(rst_dtcm),
    .sd(dtcm_ram_sd),
    .ds(dtcm_ram_ds),
    .ls(dtcm_ram_ls),
    .cs(dtcm_ram_cs),
    .we(dtcm_ram_we),
    .addr(dtcm_ram_addr),
    .wem(dtcm_ram_wem),
    .din(dtcm_ram_din),
    .dout(dtcm_ram_dout_w)
  );
  assign itcm_ram_dout = itcm_ram_dout_w;
  assign dtcm_ram_dout = dtcm_ram_dout_w;

endmodule

