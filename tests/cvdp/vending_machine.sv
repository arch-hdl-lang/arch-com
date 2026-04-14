module vending_machine (
  input logic clk,
  input logic rst,
  input logic item_button,
  input logic [2:0] item_selected,
  input logic [3:0] coin_input,
  input logic cancel,
  output logic dispense_item,
  output logic return_change,
  output logic [4:0] item_price,
  output logic [4:0] change_amount,
  output logic [2:0] dispense_item_id,
  output logic error,
  output logic return_money
);

  typedef enum logic [2:0] {
    IDLE = 3'd0,
    ITEM_SELECTION = 3'd1,
    COIN_ACCEPT = 3'd2,
    DISPENSE = 3'd3,
    CHANGE_CHECK = 3'd4,
    RETURN_CHANGE = 3'd5,
    ERROR = 3'd6
  } vending_machine_state_t;
  
  vending_machine_state_t state_r, state_next;
  
  logic [4:0] coins_accumulated;
  logic [2:0] selected_item;
  logic [4:0] selected_price;
  logic item_button_prev;
  logic cancel_prev;
  
  logic item_button_edge;
  assign item_button_edge = item_button & ~item_button_prev;
  logic cancel_edge;
  assign cancel_edge = cancel & ~cancel_prev;
  logic valid_coin;
  assign valid_coin = coin_input == 1 | coin_input == 2 | coin_input == 5 | coin_input == 10;
  logic valid_item;
  assign valid_item = item_selected == 1 | item_selected == 2 | item_selected == 3 | item_selected == 4;
  logic [5:0] new_total;
  assign new_total = 6'(6'($unsigned(coins_accumulated)) + 6'($unsigned(coin_input)));
  logic [4:0] change_val;
  assign change_val = 5'(coins_accumulated - selected_price);
  logic enough;
  assign enough = new_total >= 6'($unsigned(selected_price));
  logic [4:0] lookup_price;
  assign lookup_price = item_selected == 1 ? 5 : item_selected == 2 ? 10 : item_selected == 3 ? 15 : item_selected == 4 ? 20 : 0;
  
  always_ff @(posedge clk or posedge rst) begin
    if (rst) begin
      state_r <= IDLE;
      coins_accumulated <= 0;
      selected_item <= 0;
      selected_price <= 0;
      item_button_prev <= 1'b0;
      cancel_prev <= 1'b0;
      dispense_item <= 1'b0;
      return_change <= 1'b0;
      item_price <= 0;
      change_amount <= 0;
      dispense_item_id <= 0;
      error <= 1'b0;
      return_money <= 1'b0;
    end else begin
      state_r <= state_next;
      item_button_prev <= item_button;
      cancel_prev <= cancel;
      dispense_item <= 1'b0;
      return_change <= 1'b0;
      error <= 1'b0;
      return_money <= 1'b0;
      change_amount <= 0;
      case (state_r)
        IDLE: begin
          item_price <= 0;
          coins_accumulated <= 0;
          if (coin_input != 0) begin
            coins_accumulated <= 5'($unsigned(coin_input));
            error <= 1'b1;
          end
        end
        ITEM_SELECTION: begin
          if (cancel_edge) begin
            error <= 1'b1;
          end else if (valid_item) begin
            selected_item <= item_selected;
            selected_price <= lookup_price;
            item_price <= lookup_price;
          end else if (coin_input != 0) begin
            coins_accumulated <= 5'($unsigned(coin_input));
            error <= 1'b1;
          end
        end
        COIN_ACCEPT: begin
          item_price <= selected_price;
          if (cancel_edge) begin
            error <= 1'b1;
          end else if (coin_input != 0) begin
            if (valid_coin) begin
              coins_accumulated <= 5'(new_total);
            end else begin
              error <= 1'b1;
              return_money <= 1'b1;
              coins_accumulated <= 0;
              item_price <= 0;
            end
          end
        end
        DISPENSE: begin
          dispense_item <= 1'b1;
          dispense_item_id <= selected_item;
        end
        CHANGE_CHECK: begin
          if (coins_accumulated > selected_price) begin
            change_amount <= change_val;
          end else begin
            coins_accumulated <= 0;
            item_price <= 0;
          end
        end
        RETURN_CHANGE: begin
          return_change <= 1'b1;
          change_amount <= change_val;
          coins_accumulated <= 0;
          item_price <= 0;
        end
        ERROR: begin
          if (coins_accumulated > 0) begin
            return_money <= 1'b1;
          end
          coins_accumulated <= 0;
          item_price <= 0;
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      IDLE: begin
        if (item_button_edge) state_next = ITEM_SELECTION;
        else if (coin_input != 0) state_next = ERROR;
      end
      ITEM_SELECTION: begin
        if (cancel_edge) state_next = ERROR;
        else if (valid_item) state_next = COIN_ACCEPT;
        else if (coin_input != 0) state_next = ERROR;
      end
      COIN_ACCEPT: begin
        if (cancel_edge) state_next = ERROR;
        else if (coin_input != 0 & ~valid_coin) state_next = IDLE;
        else if (coin_input != 0 & valid_coin & enough) state_next = DISPENSE;
      end
      DISPENSE: begin
        state_next = CHANGE_CHECK;
      end
      CHANGE_CHECK: begin
        if (coins_accumulated > selected_price) state_next = RETURN_CHANGE;
        else if (~(coins_accumulated > selected_price)) state_next = IDLE;
      end
      RETURN_CHANGE: begin
        state_next = IDLE;
      end
      ERROR: begin
        state_next = IDLE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    case (state_r)
      IDLE: begin
      end
      ITEM_SELECTION: begin
      end
      COIN_ACCEPT: begin
      end
      DISPENSE: begin
      end
      CHANGE_CHECK: begin
      end
      RETURN_CHANGE: begin
      end
      ERROR: begin
      end
      default: ;
    endcase
  end
  
  // synopsys translate_off
  _auto_legal_state: assert property (@(posedge clk) !rst |-> state_r < 7)
    else $fatal(1, "FSM ILLEGAL STATE: vending_machine.state_r = %0d", state_r);
  _auto_reach_IDLE: cover property (@(posedge clk) state_r == IDLE);
  _auto_reach_ITEM_SELECTION: cover property (@(posedge clk) state_r == ITEM_SELECTION);
  _auto_reach_COIN_ACCEPT: cover property (@(posedge clk) state_r == COIN_ACCEPT);
  _auto_reach_DISPENSE: cover property (@(posedge clk) state_r == DISPENSE);
  _auto_reach_CHANGE_CHECK: cover property (@(posedge clk) state_r == CHANGE_CHECK);
  _auto_reach_RETURN_CHANGE: cover property (@(posedge clk) state_r == RETURN_CHANGE);
  _auto_reach_ERROR: cover property (@(posedge clk) state_r == ERROR);
  _auto_tr_IDLE_to_ITEM_SELECTION: cover property (@(posedge clk) state_r == IDLE && state_next == ITEM_SELECTION);
  _auto_tr_IDLE_to_ERROR: cover property (@(posedge clk) state_r == IDLE && state_next == ERROR);
  _auto_tr_ITEM_SELECTION_to_ERROR: cover property (@(posedge clk) state_r == ITEM_SELECTION && state_next == ERROR);
  _auto_tr_ITEM_SELECTION_to_COIN_ACCEPT: cover property (@(posedge clk) state_r == ITEM_SELECTION && state_next == COIN_ACCEPT);
  _auto_tr_ITEM_SELECTION_to_ERROR_1: cover property (@(posedge clk) state_r == ITEM_SELECTION && state_next == ERROR);
  _auto_tr_COIN_ACCEPT_to_ERROR: cover property (@(posedge clk) state_r == COIN_ACCEPT && state_next == ERROR);
  _auto_tr_COIN_ACCEPT_to_IDLE: cover property (@(posedge clk) state_r == COIN_ACCEPT && state_next == IDLE);
  _auto_tr_COIN_ACCEPT_to_DISPENSE: cover property (@(posedge clk) state_r == COIN_ACCEPT && state_next == DISPENSE);
  _auto_tr_DISPENSE_to_CHANGE_CHECK: cover property (@(posedge clk) state_r == DISPENSE && state_next == CHANGE_CHECK);
  _auto_tr_CHANGE_CHECK_to_RETURN_CHANGE: cover property (@(posedge clk) state_r == CHANGE_CHECK && state_next == RETURN_CHANGE);
  _auto_tr_CHANGE_CHECK_to_IDLE: cover property (@(posedge clk) state_r == CHANGE_CHECK && state_next == IDLE);
  _auto_tr_RETURN_CHANGE_to_IDLE: cover property (@(posedge clk) state_r == RETURN_CHANGE && state_next == IDLE);
  _auto_tr_ERROR_to_IDLE: cover property (@(posedge clk) state_r == ERROR && state_next == IDLE);
  // synopsys translate_on

endmodule

