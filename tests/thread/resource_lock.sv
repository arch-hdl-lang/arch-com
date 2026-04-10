module _SharedBus_Writer_0 (
  input logic clk,
  input logic rst_n,
  input logic bus_ready,
  output logic [32-1:0] bus_addr,
  output logic bus_valid,
  output logic done_0,
  output logic _shared_bus_req,
  input logic _shared_bus_grant
);

  typedef enum logic [1:0] {
    S0 = 2'd0,
    S1 = 2'd1,
    S2 = 2'd2,
    S3 = 2'd3
  } _SharedBus_Writer_0_state_t;
  
  _SharedBus_Writer_0_state_t state_r, state_next;
  
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
        if (_shared_bus_grant) state_next = S1;
      end
      S1: begin
        if (bus_ready) state_next = S2;
      end
      S2: begin
        state_next = S3;
      end
      S3: begin
        if (bus_ready) state_next = S0;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    bus_addr = 0;
    bus_valid = 0;
    done_0 = 0;
    _shared_bus_req = 0;
    case (state_r)
      S0: begin
        _shared_bus_req = 1;
      end
      S1: begin
        _shared_bus_req = 1;
        bus_valid = 1;
        bus_addr = 32'd4096;
      end
      S2: begin
        _shared_bus_req = 1;
        bus_valid = 0;
      end
      S3: begin
        done_0 = 1;
      end
      default: ;
    endcase
  end

endmodule

module _SharedBus_Writer_1 (
  input logic clk,
  input logic rst_n,
  input logic bus_ready,
  output logic [32-1:0] bus_addr,
  output logic bus_valid,
  output logic done_1,
  output logic _shared_bus_req,
  input logic _shared_bus_grant
);

  typedef enum logic [1:0] {
    S0 = 2'd0,
    S1 = 2'd1,
    S2 = 2'd2,
    S3 = 2'd3
  } _SharedBus_Writer_1_state_t;
  
  _SharedBus_Writer_1_state_t state_r, state_next;
  
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
        if (_shared_bus_grant) state_next = S1;
      end
      S1: begin
        if (bus_ready) state_next = S2;
      end
      S2: begin
        state_next = S3;
      end
      S3: begin
        if (bus_ready) state_next = S0;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    bus_addr = 0;
    bus_valid = 0;
    done_1 = 0;
    _shared_bus_req = 0;
    case (state_r)
      S0: begin
        _shared_bus_req = 1;
      end
      S1: begin
        _shared_bus_req = 1;
        bus_valid = 1;
        bus_addr = 32'd8192;
      end
      S2: begin
        _shared_bus_req = 1;
        bus_valid = 0;
      end
      S3: begin
        done_1 = 1;
      end
      default: ;
    endcase
  end

endmodule

module SharedBus #(
  parameter int NUM_CH = 2
) (
  input logic clk,
  input logic rst_n,
  output logic bus_valid,
  output logic [32-1:0] bus_addr,
  input logic bus_ready,
  output logic done_0,
  output logic done_1
);

  logic bus_valid__t1;
  logic bus_valid__t0;
  logic [32-1:0] bus_addr__t1;
  logic [32-1:0] bus_addr__t0;
  logic _shared_bus_req_1;
  logic _shared_bus_req_0;
  _SharedBus_Writer_0 _Writer_0 (
    .clk(clk),
    .rst_n(rst_n),
    .bus_ready(bus_ready),
    .bus_addr(bus_addr__t0),
    .bus_valid(bus_valid__t0),
    .done_0(done_0),
    ._shared_bus_req(_shared_bus_req_0),
    ._shared_bus_grant(_shared_bus_grant_0)
  );
  _SharedBus_Writer_1 _Writer_1 (
    .clk(clk),
    .rst_n(rst_n),
    .bus_ready(bus_ready),
    .bus_addr(bus_addr__t1),
    .bus_valid(bus_valid__t1),
    .done_1(done_1),
    ._shared_bus_req(_shared_bus_req_1),
    ._shared_bus_grant(_shared_bus_grant_1)
  );
  logic _shared_bus_grant_0;
  logic _shared_bus_grant_1;
  assign _shared_bus_grant_0 = _shared_bus_req_0;
  assign _shared_bus_grant_1 = _shared_bus_req_1 && !_shared_bus_grant_0;
  assign bus_addr = bus_addr__t0 | bus_addr__t1;
  assign bus_valid = bus_valid__t0 | bus_valid__t1;

endmodule

