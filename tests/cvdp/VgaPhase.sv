package VgaPhasePkg;
  typedef enum logic [1:0] {
    ST_ACTIVE = 2'd0,
    ST_FRONT = 2'd1,
    ST_PULSE = 2'd2,
    ST_BACK = 2'd3
  } VgaPhase;
  
endpackage

