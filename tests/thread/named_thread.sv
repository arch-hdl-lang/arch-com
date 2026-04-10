module _NamedThreadDemo_Writer (
  input logic clk,
  input logic rst_n,
  input logic wr_ack,
  output logic wr_en
);

  typedef enum logic [0:0] {
    S0 = 1'd0
  } _NamedThreadDemo_Writer_state_t;
  
  _NamedThreadDemo_Writer_state_t state_r, state_next;
  
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      state_r <= S0;
    end else begin
      state_r <= state_next;
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      S0: begin
        if (wr_ack) state_next = S0;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    wr_en = 0;
    case (state_r)
      S0: begin
        wr_en = 1;
      end
      default: ;
    endcase
  end

endmodule

module _NamedThreadDemo_Reader (
  input logic clk,
  input logic rst_n,
  input logic rd_ack,
  output logic rd_en
);

  typedef enum logic [0:0] {
    S0 = 1'd0
  } _NamedThreadDemo_Reader_state_t;
  
  _NamedThreadDemo_Reader_state_t state_r, state_next;
  
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      state_r <= S0;
    end else begin
      state_r <= state_next;
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      S0: begin
        if (rd_ack) state_next = S0;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    rd_en = 0;
    case (state_r)
      S0: begin
        rd_en = 1;
      end
      default: ;
    endcase
  end

endmodule

module NamedThreadDemo (
  input logic clk,
  input logic rst_n,
  output logic wr_en,
  output logic rd_en,
  input logic wr_ack,
  input logic rd_ack
);

  _NamedThreadDemo_Writer _Writer (
    .clk(clk),
    .rst_n(rst_n),
    .wr_ack(wr_ack),
    .wr_en(wr_en)
  );
  _NamedThreadDemo_Reader _Reader (
    .clk(clk),
    .rst_n(rst_n),
    .rd_ack(rd_ack),
    .rd_en(rd_en)
  );

endmodule

