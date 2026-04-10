module _BasicThread_thread (
  input logic clk,
  input logic rst_n,
  input logic ar_ready,
  input logic [32-1:0] r_data,
  input logic r_valid,
  output logic [32-1:0] ar_addr,
  output logic ar_valid,
  output logic r_ready,
  output logic [32-1:0] data_r
);

  typedef enum logic [1:0] {
    S0 = 2'd0,
    S1 = 2'd1,
    S2 = 2'd2
  } _BasicThread_thread_state_t;
  
  _BasicThread_thread_state_t state_r, state_next;
  
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      state_r <= S0;
      data_r <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        S2: begin
          data_r <= r_data;
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      S0: begin
        if (ar_ready) state_next = S1;
      end
      S1: begin
        if (r_valid) state_next = S2;
      end
      S2: begin
        state_next = S0;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    ar_addr = 0;
    ar_valid = 0;
    r_ready = 0;
    case (state_r)
      S0: begin
        ar_valid = 1;
        ar_addr = 32'd100;
      end
      S1: begin
        r_ready = 1;
      end
      S2: begin
      end
      default: ;
    endcase
  end

endmodule

module BasicThread (
  input logic clk,
  input logic rst_n,
  output logic ar_valid,
  output logic [32-1:0] ar_addr,
  input logic ar_ready,
  output logic r_ready,
  input logic r_valid,
  input logic [32-1:0] r_data,
  output logic [32-1:0] data_out
);

  logic [32-1:0] data_r;
  assign data_out = data_r;
  _BasicThread_thread _thread (
    .clk(clk),
    .rst_n(rst_n),
    .ar_ready(ar_ready),
    .r_data(r_data),
    .r_valid(r_valid),
    .ar_addr(ar_addr),
    .ar_valid(ar_valid),
    .r_ready(r_ready),
    .data_r(data_r)
  );

endmodule

