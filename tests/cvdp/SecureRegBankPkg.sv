package SecureRegBankPkg;
  typedef enum logic [1:0] {
    LOCKED = 2'd0,
    GOT_CODE0 = 2'd1,
    UNLOCKED = 2'd2
  } LockState;
  
endpackage

