// Verilated -*- C++ -*-
// DESCRIPTION: Verilator output: Design implementation internals
// See VAesCipherTop.h for the primary calling header

#include "VAesCipherTop__pch.h"
#include "VAesCipherTop___024root.h"

VL_ATTR_COLD void VAesCipherTop___024root___stl_sequent__TOP__1(VAesCipherTop___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VAesCipherTop___024root___stl_sequent__TOP__1\n"); );
    VAesCipherTop__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Init
    CData/*7:0*/ AesCipherTop__DOT__sa30_sr;
    AesCipherTop__DOT__sa30_sr = 0;
    CData/*7:0*/ AesCipherTop__DOT__sa33_sr;
    AesCipherTop__DOT__sa33_sr = 0;
    CData/*7:0*/ __Vfunc_AesCipherTop__DOT__AesSbox__12__Vfuncout;
    __Vfunc_AesCipherTop__DOT__AesSbox__12__Vfuncout = 0;
    CData/*7:0*/ __Vfunc_AesCipherTop__DOT__AesSbox__12__a;
    __Vfunc_AesCipherTop__DOT__AesSbox__12__a = 0;
    CData/*7:0*/ __Vfunc_AesCipherTop__DOT__AesSbox__13__Vfuncout;
    __Vfunc_AesCipherTop__DOT__AesSbox__13__Vfuncout = 0;
    CData/*7:0*/ __Vfunc_AesCipherTop__DOT__AesSbox__13__a;
    __Vfunc_AesCipherTop__DOT__AesSbox__13__a = 0;
    CData/*7:0*/ __Vfunc_AesCipherTop__DOT__AesSbox__14__Vfuncout;
    __Vfunc_AesCipherTop__DOT__AesSbox__14__Vfuncout = 0;
    CData/*7:0*/ __Vfunc_AesCipherTop__DOT__AesSbox__14__a;
    __Vfunc_AesCipherTop__DOT__AesSbox__14__a = 0;
    CData/*7:0*/ __Vfunc_AesCipherTop__DOT__AesSbox__15__a;
    __Vfunc_AesCipherTop__DOT__AesSbox__15__a = 0;
    CData/*31:0*/ __Vdeeptemp_h2b1c42e0__0;
    CData/*31:0*/ __Vdeeptemp_h2acd4024__0;
    CData/*31:0*/ __Vdeeptemp_h4fb68fd8__0;
    CData/*31:0*/ __Vdeeptemp_hfded1fc4__0;
    CData/*31:0*/ __Vdeeptemp_h377c8bb0__0;
    CData/*31:0*/ __Vdeeptemp_h4ba552b1__0;
    CData/*31:0*/ __Vdeeptemp_h08d8f6a2__0;
    CData/*31:0*/ __Vdeeptemp_h24ee4228__0;
    CData/*31:0*/ __Vdeeptemp_hafb8d4b2__0;
    CData/*31:0*/ __Vdeeptemp_h362e9fff__0;
    CData/*31:0*/ __Vdeeptemp_hbcda1274__0;
    CData/*31:0*/ __Vdeeptemp_h1fde1343__0;
    // Body
    vlSelfRef.AesCipherTop__DOT__sa23_sr = vlSelfRef.__Vfunc_AesCipherTop__DOT__AesSbox__11__Vfuncout;
    __Vfunc_AesCipherTop__DOT__AesSbox__12__a = vlSelfRef.AesCipherTop__DOT__sa33;
    __Vdeeptemp_h2b1c42e0__0 = ((0x89U == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                 ? 0xa7U : ((0x8aU 
                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                             ? 0x7eU
                                             : ((0x8bU 
                                                 == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                 ? 0x3dU
                                                 : 
                                                ((0x8cU 
                                                  == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                  ? 0x64U
                                                  : 
                                                 ((0x8dU 
                                                   == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                   ? 0x5dU
                                                   : 
                                                  ((0x8eU 
                                                    == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                    ? 0x19U
                                                    : 
                                                   ((0x8fU 
                                                     == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                     ? 0x73U
                                                     : 
                                                    ((0x90U 
                                                      == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                      ? 0x60U
                                                      : 
                                                     ((0x91U 
                                                       == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                       ? 0x81U
                                                       : 
                                                      ((0x92U 
                                                        == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                        ? 0x4fU
                                                        : 
                                                       ((0x93U 
                                                         == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                         ? 0xdcU
                                                         : 
                                                        ((0x94U 
                                                          == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                          ? 0x22U
                                                          : 
                                                         ((0x95U 
                                                           == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                           ? 0x2aU
                                                           : 
                                                          ((0x96U 
                                                            == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                            ? 0x90U
                                                            : 
                                                           ((0x97U 
                                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                             ? 0x88U
                                                             : 
                                                            ((0x98U 
                                                              == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                              ? 0x46U
                                                              : 
                                                             ((0x99U 
                                                               == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                               ? 0xeeU
                                                               : 
                                                              ((0x9aU 
                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                ? 0xb8U
                                                                : 
                                                               ((0x9bU 
                                                                 == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                 ? 0x14U
                                                                 : 
                                                                ((0x9cU 
                                                                  == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                  ? 0xdeU
                                                                  : 
                                                                 ((0x9dU 
                                                                   == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                   ? 0x5eU
                                                                   : 
                                                                  ((0x9eU 
                                                                    == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                    ? 0xbU
                                                                    : 
                                                                   ((0x9fU 
                                                                     == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                     ? 0xdbU
                                                                     : 
                                                                    ((0xa0U 
                                                                      == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                      ? 0xe0U
                                                                      : 
                                                                     ((0xa1U 
                                                                       == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                       ? 0x32U
                                                                       : 
                                                                      ((0xa2U 
                                                                        == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                        ? 0x3aU
                                                                        : 
                                                                       ((0xa3U 
                                                                         == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                         ? 0xaU
                                                                         : 
                                                                        ((0xa4U 
                                                                          == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                          ? 0x49U
                                                                          : 
                                                                         ((0xa5U 
                                                                           == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                           ? 6U
                                                                           : 
                                                                          ((0xa6U 
                                                                            == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                            ? 0x24U
                                                                            : 
                                                                           ((0xa7U 
                                                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                             ? 0x5cU
                                                                             : 
                                                                            ((0xa8U 
                                                                              == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                              ? 0xc2U
                                                                              : 
                                                                             ((0xa9U 
                                                                               == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                               ? 0xd3U
                                                                               : 
                                                                              ((0xaaU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                ? 0xacU
                                                                                : 
                                                                               ((0xabU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x62U
                                                                                 : 
                                                                                ((0xacU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x91U
                                                                                 : 
                                                                                ((0xadU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x95U
                                                                                 : 
                                                                                ((0xaeU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xe4U
                                                                                 : 
                                                                                ((0xafU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x79U
                                                                                 : 
                                                                                ((0xb0U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xe7U
                                                                                 : 
                                                                                ((0xb1U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xc8U
                                                                                 : 
                                                                                ((0xb2U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x37U
                                                                                 : 
                                                                                ((0xb3U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x6dU
                                                                                 : 
                                                                                ((0xb4U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x8dU
                                                                                 : 
                                                                                ((0xb5U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xd5U
                                                                                 : 
                                                                                ((0xb6U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x4eU
                                                                                 : 
                                                                                ((0xb7U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xa9U
                                                                                 : 
                                                                                ((0xb8U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x6cU
                                                                                 : 
                                                                                ((0xb9U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x56U
                                                                                 : 
                                                                                ((0xbaU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xf4U
                                                                                 : 
                                                                                ((0xbbU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xeaU
                                                                                 : 
                                                                                ((0xbcU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x65U
                                                                                 : 
                                                                                ((0xbdU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x7aU
                                                                                 : 
                                                                                ((0xbeU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xaeU
                                                                                 : 
                                                                                ((0xbfU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 8U
                                                                                 : 
                                                                                ((0xc0U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xbaU
                                                                                 : 
                                                                                ((0xc1U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x78U
                                                                                 : 
                                                                                ((0xc2U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x25U
                                                                                 : 
                                                                                ((0xc3U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x2eU
                                                                                 : 
                                                                                ((0xc4U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x1cU
                                                                                 : 
                                                                                ((0xc5U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xa6U
                                                                                 : 
                                                                                ((0xc6U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xb4U
                                                                                 : 
                                                                                ((0xc7U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xc6U
                                                                                 : 
                                                                                ((0xc8U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xe8U
                                                                                 : 
                                                                                ((0xc9U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xddU
                                                                                 : 
                                                                                ((0xcaU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x74U
                                                                                 : 
                                                                                ((0xcbU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x1fU
                                                                                 : 
                                                                                ((0xccU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x4bU
                                                                                 : 
                                                                                ((0xcdU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xbdU
                                                                                 : 
                                                                                ((0xceU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x8bU
                                                                                 : 
                                                                                ((0xcfU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x8aU
                                                                                 : 
                                                                                ((0xd0U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x70U
                                                                                 : 
                                                                                ((0xd1U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x3eU
                                                                                 : 
                                                                                ((0xd2U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xb5U
                                                                                 : 
                                                                                ((0xd3U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x66U
                                                                                 : 
                                                                                ((0xd4U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x48U
                                                                                 : 
                                                                                ((0xd5U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 3U
                                                                                 : 
                                                                                ((0xd6U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xf6U
                                                                                 : 
                                                                                ((0xd7U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xeU
                                                                                 : 
                                                                                ((0xd8U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x61U
                                                                                 : 
                                                                                ((0xd9U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x35U
                                                                                 : 
                                                                                ((0xdaU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x57U
                                                                                 : 
                                                                                ((0xdbU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xb9U
                                                                                 : 
                                                                                ((0xdcU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x86U
                                                                                 : 
                                                                                ((0xddU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xc1U
                                                                                 : 
                                                                                ((0xdeU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x1dU
                                                                                 : 
                                                                                ((0xdfU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x9eU
                                                                                 : 
                                                                                ((0xe0U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xe1U
                                                                                 : 
                                                                                ((0xe1U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xf8U
                                                                                 : 
                                                                                ((0xe2U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x98U
                                                                                 : 
                                                                                ((0xe3U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x11U
                                                                                 : 
                                                                                ((0xe4U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x69U
                                                                                 : 
                                                                                ((0xe5U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xd9U
                                                                                 : 
                                                                                ((0xe6U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x8eU
                                                                                 : 
                                                                                ((0xe7U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x94U
                                                                                 : 
                                                                                ((0xe8U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x9bU
                                                                                 : 
                                                                                ((0xe9U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x1eU
                                                                                 : 
                                                                                ((0xeaU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x87U
                                                                                 : 
                                                                                ((0xebU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xe9U
                                                                                 : 
                                                                                ((0xecU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xceU
                                                                                 : 
                                                                                ((0xedU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x55U
                                                                                 : 
                                                                                ((0xeeU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x28U
                                                                                 : 
                                                                                ((0xefU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xdfU
                                                                                 : 
                                                                                ((0xf0U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x8cU
                                                                                 : 
                                                                                ((0xf1U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xa1U
                                                                                 : 
                                                                                ((0xf2U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x89U
                                                                                 : 
                                                                                ((0xf3U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xdU
                                                                                 : 
                                                                                ((0xf4U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xbfU
                                                                                 : 
                                                                                ((0xf5U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xe6U
                                                                                 : 
                                                                                ((0xf6U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x42U
                                                                                 : 
                                                                                ((0xf7U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x68U
                                                                                 : 
                                                                                ((0xf8U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x41U
                                                                                 : 
                                                                                ((0xf9U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x99U
                                                                                 : 
                                                                                ((0xfaU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x2dU
                                                                                 : 
                                                                                ((0xfbU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xfU
                                                                                 : 
                                                                                ((0xfcU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xb0U
                                                                                 : 
                                                                                ((0xfdU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x54U
                                                                                 : 
                                                                                ((0xfeU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xbbU
                                                                                 : 
                                                                                ((0xffU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x16U
                                                                                 : 0U)))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))));
    __Vdeeptemp_h4fb68fd8__0 = ((0x12U == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                 ? 0xc9U : ((0x13U 
                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                             ? 0x7dU
                                             : ((0x14U 
                                                 == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                 ? 0xfaU
                                                 : 
                                                ((0x15U 
                                                  == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                  ? 0x59U
                                                  : 
                                                 ((0x16U 
                                                   == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                   ? 0x47U
                                                   : 
                                                  ((0x17U 
                                                    == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                    ? 0xf0U
                                                    : 
                                                   ((0x18U 
                                                     == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                     ? 0xadU
                                                     : 
                                                    ((0x19U 
                                                      == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                      ? 0xd4U
                                                      : 
                                                     ((0x1aU 
                                                       == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                       ? 0xa2U
                                                       : 
                                                      ((0x1bU 
                                                        == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                        ? 0xafU
                                                        : 
                                                       ((0x1cU 
                                                         == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                         ? 0x9cU
                                                         : 
                                                        ((0x1dU 
                                                          == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                          ? 0xa4U
                                                          : 
                                                         ((0x1eU 
                                                           == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                           ? 0x72U
                                                           : 
                                                          ((0x1fU 
                                                            == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                            ? 0xc0U
                                                            : 
                                                           ((0x20U 
                                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                             ? 0xb7U
                                                             : 
                                                            ((0x21U 
                                                              == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                              ? 0xfdU
                                                              : 
                                                             ((0x22U 
                                                               == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                               ? 0x93U
                                                               : 
                                                              ((0x23U 
                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                ? 0x26U
                                                                : 
                                                               ((0x24U 
                                                                 == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                 ? 0x36U
                                                                 : 
                                                                ((0x25U 
                                                                  == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                  ? 0x3fU
                                                                  : 
                                                                 ((0x26U 
                                                                   == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                   ? 0xf7U
                                                                   : 
                                                                  ((0x27U 
                                                                    == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                    ? 0xccU
                                                                    : 
                                                                   ((0x28U 
                                                                     == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                     ? 0x34U
                                                                     : 
                                                                    ((0x29U 
                                                                      == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                      ? 0xa5U
                                                                      : 
                                                                     ((0x2aU 
                                                                       == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                       ? 0xe5U
                                                                       : 
                                                                      ((0x2bU 
                                                                        == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                        ? 0xf1U
                                                                        : 
                                                                       ((0x2cU 
                                                                         == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                         ? 0x71U
                                                                         : 
                                                                        ((0x2dU 
                                                                          == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                          ? 0xd8U
                                                                          : 
                                                                         ((0x2eU 
                                                                           == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                           ? 0x31U
                                                                           : 
                                                                          ((0x2fU 
                                                                            == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                            ? 0x15U
                                                                            : 
                                                                           ((0x30U 
                                                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                             ? 4U
                                                                             : 
                                                                            ((0x31U 
                                                                              == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                              ? 0xc7U
                                                                              : 
                                                                             ((0x32U 
                                                                               == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                               ? 0x23U
                                                                               : 
                                                                              ((0x33U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                ? 0xc3U
                                                                                : 
                                                                               ((0x34U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x18U
                                                                                 : 
                                                                                ((0x35U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x96U
                                                                                 : 
                                                                                ((0x36U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 5U
                                                                                 : 
                                                                                ((0x37U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x9aU
                                                                                 : 
                                                                                ((0x38U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 7U
                                                                                 : 
                                                                                ((0x39U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x12U
                                                                                 : 
                                                                                ((0x3aU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x80U
                                                                                 : 
                                                                                ((0x3bU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xe2U
                                                                                 : 
                                                                                ((0x3cU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xebU
                                                                                 : 
                                                                                ((0x3dU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x27U
                                                                                 : 
                                                                                ((0x3eU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xb2U
                                                                                 : 
                                                                                ((0x3fU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x75U
                                                                                 : 
                                                                                ((0x40U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 9U
                                                                                 : 
                                                                                ((0x41U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x83U
                                                                                 : 
                                                                                ((0x42U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x2cU
                                                                                 : 
                                                                                ((0x43U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x1aU
                                                                                 : 
                                                                                ((0x44U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x1bU
                                                                                 : 
                                                                                ((0x45U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x6eU
                                                                                 : 
                                                                                ((0x46U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x5aU
                                                                                 : 
                                                                                ((0x47U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xa0U
                                                                                 : 
                                                                                ((0x48U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x52U
                                                                                 : 
                                                                                ((0x49U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x3bU
                                                                                 : 
                                                                                ((0x4aU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xd6U
                                                                                 : 
                                                                                ((0x4bU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xb3U
                                                                                 : 
                                                                                ((0x4cU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x29U
                                                                                 : 
                                                                                ((0x4dU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xe3U
                                                                                 : 
                                                                                ((0x4eU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x2fU
                                                                                 : 
                                                                                ((0x4fU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x84U
                                                                                 : 
                                                                                ((0x50U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x53U
                                                                                 : 
                                                                                ((0x51U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xd1U
                                                                                 : 
                                                                                ((0x52U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0U
                                                                                 : 
                                                                                ((0x53U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xedU
                                                                                 : 
                                                                                ((0x54U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x20U
                                                                                 : 
                                                                                ((0x55U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xfcU
                                                                                 : 
                                                                                ((0x56U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xb1U
                                                                                 : 
                                                                                ((0x57U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x5bU
                                                                                 : 
                                                                                ((0x58U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x6aU
                                                                                 : 
                                                                                ((0x59U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xcbU
                                                                                 : 
                                                                                ((0x5aU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xbeU
                                                                                 : 
                                                                                ((0x5bU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x39U
                                                                                 : 
                                                                                ((0x5cU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x4aU
                                                                                 : 
                                                                                ((0x5dU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x4cU
                                                                                 : 
                                                                                ((0x5eU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x58U
                                                                                 : 
                                                                                ((0x5fU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xcfU
                                                                                 : 
                                                                                ((0x60U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xd0U
                                                                                 : 
                                                                                ((0x61U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xefU
                                                                                 : 
                                                                                ((0x62U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xaaU
                                                                                 : 
                                                                                ((0x63U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xfbU
                                                                                 : 
                                                                                ((0x64U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x43U
                                                                                 : 
                                                                                ((0x65U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x4dU
                                                                                 : 
                                                                                ((0x66U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x33U
                                                                                 : 
                                                                                ((0x67U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x85U
                                                                                 : 
                                                                                ((0x68U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x45U
                                                                                 : 
                                                                                ((0x69U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xf9U
                                                                                 : 
                                                                                ((0x6aU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 2U
                                                                                 : 
                                                                                ((0x6bU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x7fU
                                                                                 : 
                                                                                ((0x6cU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x50U
                                                                                 : 
                                                                                ((0x6dU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x3cU
                                                                                 : 
                                                                                ((0x6eU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x9fU
                                                                                 : 
                                                                                ((0x6fU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xa8U
                                                                                 : 
                                                                                ((0x70U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x51U
                                                                                 : 
                                                                                ((0x71U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xa3U
                                                                                 : 
                                                                                ((0x72U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x40U
                                                                                 : 
                                                                                ((0x73U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x8fU
                                                                                 : 
                                                                                ((0x74U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x92U
                                                                                 : 
                                                                                ((0x75U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x9dU
                                                                                 : 
                                                                                ((0x76U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x38U
                                                                                 : 
                                                                                ((0x77U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xf5U
                                                                                 : 
                                                                                ((0x78U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xbcU
                                                                                 : 
                                                                                ((0x79U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xb6U
                                                                                 : 
                                                                                ((0x7aU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xdaU
                                                                                 : 
                                                                                ((0x7bU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x21U
                                                                                 : 
                                                                                ((0x7cU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x10U
                                                                                 : 
                                                                                ((0x7dU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xffU
                                                                                 : 
                                                                                ((0x7eU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xf3U
                                                                                 : 
                                                                                ((0x7fU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xd2U
                                                                                 : 
                                                                                ((0x80U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xcdU
                                                                                 : 
                                                                                ((0x81U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xcU
                                                                                 : 
                                                                                ((0x82U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x13U
                                                                                 : 
                                                                                ((0x83U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xecU
                                                                                 : 
                                                                                ((0x84U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x5fU
                                                                                 : 
                                                                                ((0x85U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x97U
                                                                                 : 
                                                                                ((0x86U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x44U
                                                                                 : 
                                                                                ((0x87U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0x17U
                                                                                 : 
                                                                                ((0x88U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                                 ? 0xc4U
                                                                                 : __Vdeeptemp_h2b1c42e0__0)))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))));
    __Vdeeptemp_h2acd4024__0 = ((0x11U == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                 ? 0x82U : __Vdeeptemp_h4fb68fd8__0);
    __Vfunc_AesCipherTop__DOT__AesSbox__12__Vfuncout 
        = ((0U == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
            ? 0x63U : ((1U == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                        ? 0x7cU : ((2U == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                    ? 0x77U : ((3U 
                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                ? 0x7bU
                                                : (
                                                   (4U 
                                                    == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                    ? 0xf2U
                                                    : 
                                                   ((5U 
                                                     == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                     ? 0x6bU
                                                     : 
                                                    ((6U 
                                                      == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                      ? 0x6fU
                                                      : 
                                                     ((7U 
                                                       == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                       ? 0xc5U
                                                       : 
                                                      ((8U 
                                                        == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                        ? 0x30U
                                                        : 
                                                       ((9U 
                                                         == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                         ? 1U
                                                         : 
                                                        ((0xaU 
                                                          == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                          ? 0x67U
                                                          : 
                                                         ((0xbU 
                                                           == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                           ? 0x2bU
                                                           : 
                                                          ((0xcU 
                                                            == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                            ? 0xfeU
                                                            : 
                                                           ((0xdU 
                                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                             ? 0xd7U
                                                             : 
                                                            ((0xeU 
                                                              == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                              ? 0xabU
                                                              : 
                                                             ((0xfU 
                                                               == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                               ? 0x76U
                                                               : 
                                                              ((0x10U 
                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__12__a))
                                                                ? 0xcaU
                                                                : __Vdeeptemp_h2acd4024__0)))))))))))))))));
    AesCipherTop__DOT__sa30_sr = __Vfunc_AesCipherTop__DOT__AesSbox__12__Vfuncout;
    __Vfunc_AesCipherTop__DOT__AesSbox__13__a = vlSelfRef.AesCipherTop__DOT__sa30;
    __Vdeeptemp_hfded1fc4__0 = ((0x89U == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                 ? 0xa7U : ((0x8aU 
                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                             ? 0x7eU
                                             : ((0x8bU 
                                                 == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                 ? 0x3dU
                                                 : 
                                                ((0x8cU 
                                                  == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                  ? 0x64U
                                                  : 
                                                 ((0x8dU 
                                                   == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                   ? 0x5dU
                                                   : 
                                                  ((0x8eU 
                                                    == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                    ? 0x19U
                                                    : 
                                                   ((0x8fU 
                                                     == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                     ? 0x73U
                                                     : 
                                                    ((0x90U 
                                                      == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                      ? 0x60U
                                                      : 
                                                     ((0x91U 
                                                       == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                       ? 0x81U
                                                       : 
                                                      ((0x92U 
                                                        == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                        ? 0x4fU
                                                        : 
                                                       ((0x93U 
                                                         == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                         ? 0xdcU
                                                         : 
                                                        ((0x94U 
                                                          == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                          ? 0x22U
                                                          : 
                                                         ((0x95U 
                                                           == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                           ? 0x2aU
                                                           : 
                                                          ((0x96U 
                                                            == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                            ? 0x90U
                                                            : 
                                                           ((0x97U 
                                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                             ? 0x88U
                                                             : 
                                                            ((0x98U 
                                                              == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                              ? 0x46U
                                                              : 
                                                             ((0x99U 
                                                               == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                               ? 0xeeU
                                                               : 
                                                              ((0x9aU 
                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                ? 0xb8U
                                                                : 
                                                               ((0x9bU 
                                                                 == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                 ? 0x14U
                                                                 : 
                                                                ((0x9cU 
                                                                  == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                  ? 0xdeU
                                                                  : 
                                                                 ((0x9dU 
                                                                   == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                   ? 0x5eU
                                                                   : 
                                                                  ((0x9eU 
                                                                    == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                    ? 0xbU
                                                                    : 
                                                                   ((0x9fU 
                                                                     == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                     ? 0xdbU
                                                                     : 
                                                                    ((0xa0U 
                                                                      == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                      ? 0xe0U
                                                                      : 
                                                                     ((0xa1U 
                                                                       == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                       ? 0x32U
                                                                       : 
                                                                      ((0xa2U 
                                                                        == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                        ? 0x3aU
                                                                        : 
                                                                       ((0xa3U 
                                                                         == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                         ? 0xaU
                                                                         : 
                                                                        ((0xa4U 
                                                                          == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                          ? 0x49U
                                                                          : 
                                                                         ((0xa5U 
                                                                           == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                           ? 6U
                                                                           : 
                                                                          ((0xa6U 
                                                                            == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                            ? 0x24U
                                                                            : 
                                                                           ((0xa7U 
                                                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                             ? 0x5cU
                                                                             : 
                                                                            ((0xa8U 
                                                                              == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                              ? 0xc2U
                                                                              : 
                                                                             ((0xa9U 
                                                                               == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                               ? 0xd3U
                                                                               : 
                                                                              ((0xaaU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                ? 0xacU
                                                                                : 
                                                                               ((0xabU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x62U
                                                                                 : 
                                                                                ((0xacU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x91U
                                                                                 : 
                                                                                ((0xadU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x95U
                                                                                 : 
                                                                                ((0xaeU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xe4U
                                                                                 : 
                                                                                ((0xafU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x79U
                                                                                 : 
                                                                                ((0xb0U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xe7U
                                                                                 : 
                                                                                ((0xb1U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xc8U
                                                                                 : 
                                                                                ((0xb2U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x37U
                                                                                 : 
                                                                                ((0xb3U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x6dU
                                                                                 : 
                                                                                ((0xb4U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x8dU
                                                                                 : 
                                                                                ((0xb5U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xd5U
                                                                                 : 
                                                                                ((0xb6U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x4eU
                                                                                 : 
                                                                                ((0xb7U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xa9U
                                                                                 : 
                                                                                ((0xb8U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x6cU
                                                                                 : 
                                                                                ((0xb9U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x56U
                                                                                 : 
                                                                                ((0xbaU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xf4U
                                                                                 : 
                                                                                ((0xbbU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xeaU
                                                                                 : 
                                                                                ((0xbcU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x65U
                                                                                 : 
                                                                                ((0xbdU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x7aU
                                                                                 : 
                                                                                ((0xbeU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xaeU
                                                                                 : 
                                                                                ((0xbfU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 8U
                                                                                 : 
                                                                                ((0xc0U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xbaU
                                                                                 : 
                                                                                ((0xc1U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x78U
                                                                                 : 
                                                                                ((0xc2U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x25U
                                                                                 : 
                                                                                ((0xc3U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x2eU
                                                                                 : 
                                                                                ((0xc4U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x1cU
                                                                                 : 
                                                                                ((0xc5U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xa6U
                                                                                 : 
                                                                                ((0xc6U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xb4U
                                                                                 : 
                                                                                ((0xc7U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xc6U
                                                                                 : 
                                                                                ((0xc8U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xe8U
                                                                                 : 
                                                                                ((0xc9U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xddU
                                                                                 : 
                                                                                ((0xcaU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x74U
                                                                                 : 
                                                                                ((0xcbU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x1fU
                                                                                 : 
                                                                                ((0xccU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x4bU
                                                                                 : 
                                                                                ((0xcdU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xbdU
                                                                                 : 
                                                                                ((0xceU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x8bU
                                                                                 : 
                                                                                ((0xcfU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x8aU
                                                                                 : 
                                                                                ((0xd0U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x70U
                                                                                 : 
                                                                                ((0xd1U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x3eU
                                                                                 : 
                                                                                ((0xd2U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xb5U
                                                                                 : 
                                                                                ((0xd3U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x66U
                                                                                 : 
                                                                                ((0xd4U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x48U
                                                                                 : 
                                                                                ((0xd5U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 3U
                                                                                 : 
                                                                                ((0xd6U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xf6U
                                                                                 : 
                                                                                ((0xd7U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xeU
                                                                                 : 
                                                                                ((0xd8U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x61U
                                                                                 : 
                                                                                ((0xd9U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x35U
                                                                                 : 
                                                                                ((0xdaU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x57U
                                                                                 : 
                                                                                ((0xdbU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xb9U
                                                                                 : 
                                                                                ((0xdcU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x86U
                                                                                 : 
                                                                                ((0xddU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xc1U
                                                                                 : 
                                                                                ((0xdeU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x1dU
                                                                                 : 
                                                                                ((0xdfU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x9eU
                                                                                 : 
                                                                                ((0xe0U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xe1U
                                                                                 : 
                                                                                ((0xe1U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xf8U
                                                                                 : 
                                                                                ((0xe2U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x98U
                                                                                 : 
                                                                                ((0xe3U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x11U
                                                                                 : 
                                                                                ((0xe4U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x69U
                                                                                 : 
                                                                                ((0xe5U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xd9U
                                                                                 : 
                                                                                ((0xe6U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x8eU
                                                                                 : 
                                                                                ((0xe7U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x94U
                                                                                 : 
                                                                                ((0xe8U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x9bU
                                                                                 : 
                                                                                ((0xe9U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x1eU
                                                                                 : 
                                                                                ((0xeaU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x87U
                                                                                 : 
                                                                                ((0xebU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xe9U
                                                                                 : 
                                                                                ((0xecU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xceU
                                                                                 : 
                                                                                ((0xedU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x55U
                                                                                 : 
                                                                                ((0xeeU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x28U
                                                                                 : 
                                                                                ((0xefU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xdfU
                                                                                 : 
                                                                                ((0xf0U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x8cU
                                                                                 : 
                                                                                ((0xf1U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xa1U
                                                                                 : 
                                                                                ((0xf2U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x89U
                                                                                 : 
                                                                                ((0xf3U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xdU
                                                                                 : 
                                                                                ((0xf4U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xbfU
                                                                                 : 
                                                                                ((0xf5U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xe6U
                                                                                 : 
                                                                                ((0xf6U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x42U
                                                                                 : 
                                                                                ((0xf7U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x68U
                                                                                 : 
                                                                                ((0xf8U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x41U
                                                                                 : 
                                                                                ((0xf9U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x99U
                                                                                 : 
                                                                                ((0xfaU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x2dU
                                                                                 : 
                                                                                ((0xfbU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xfU
                                                                                 : 
                                                                                ((0xfcU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xb0U
                                                                                 : 
                                                                                ((0xfdU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x54U
                                                                                 : 
                                                                                ((0xfeU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xbbU
                                                                                 : 
                                                                                ((0xffU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x16U
                                                                                 : 0U)))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))));
    __Vdeeptemp_h4ba552b1__0 = ((0x12U == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                 ? 0xc9U : ((0x13U 
                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                             ? 0x7dU
                                             : ((0x14U 
                                                 == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                 ? 0xfaU
                                                 : 
                                                ((0x15U 
                                                  == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                  ? 0x59U
                                                  : 
                                                 ((0x16U 
                                                   == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                   ? 0x47U
                                                   : 
                                                  ((0x17U 
                                                    == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                    ? 0xf0U
                                                    : 
                                                   ((0x18U 
                                                     == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                     ? 0xadU
                                                     : 
                                                    ((0x19U 
                                                      == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                      ? 0xd4U
                                                      : 
                                                     ((0x1aU 
                                                       == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                       ? 0xa2U
                                                       : 
                                                      ((0x1bU 
                                                        == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                        ? 0xafU
                                                        : 
                                                       ((0x1cU 
                                                         == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                         ? 0x9cU
                                                         : 
                                                        ((0x1dU 
                                                          == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                          ? 0xa4U
                                                          : 
                                                         ((0x1eU 
                                                           == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                           ? 0x72U
                                                           : 
                                                          ((0x1fU 
                                                            == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                            ? 0xc0U
                                                            : 
                                                           ((0x20U 
                                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                             ? 0xb7U
                                                             : 
                                                            ((0x21U 
                                                              == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                              ? 0xfdU
                                                              : 
                                                             ((0x22U 
                                                               == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                               ? 0x93U
                                                               : 
                                                              ((0x23U 
                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                ? 0x26U
                                                                : 
                                                               ((0x24U 
                                                                 == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                 ? 0x36U
                                                                 : 
                                                                ((0x25U 
                                                                  == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                  ? 0x3fU
                                                                  : 
                                                                 ((0x26U 
                                                                   == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                   ? 0xf7U
                                                                   : 
                                                                  ((0x27U 
                                                                    == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                    ? 0xccU
                                                                    : 
                                                                   ((0x28U 
                                                                     == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                     ? 0x34U
                                                                     : 
                                                                    ((0x29U 
                                                                      == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                      ? 0xa5U
                                                                      : 
                                                                     ((0x2aU 
                                                                       == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                       ? 0xe5U
                                                                       : 
                                                                      ((0x2bU 
                                                                        == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                        ? 0xf1U
                                                                        : 
                                                                       ((0x2cU 
                                                                         == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                         ? 0x71U
                                                                         : 
                                                                        ((0x2dU 
                                                                          == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                          ? 0xd8U
                                                                          : 
                                                                         ((0x2eU 
                                                                           == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                           ? 0x31U
                                                                           : 
                                                                          ((0x2fU 
                                                                            == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                            ? 0x15U
                                                                            : 
                                                                           ((0x30U 
                                                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                             ? 4U
                                                                             : 
                                                                            ((0x31U 
                                                                              == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                              ? 0xc7U
                                                                              : 
                                                                             ((0x32U 
                                                                               == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                               ? 0x23U
                                                                               : 
                                                                              ((0x33U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                ? 0xc3U
                                                                                : 
                                                                               ((0x34U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x18U
                                                                                 : 
                                                                                ((0x35U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x96U
                                                                                 : 
                                                                                ((0x36U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 5U
                                                                                 : 
                                                                                ((0x37U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x9aU
                                                                                 : 
                                                                                ((0x38U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 7U
                                                                                 : 
                                                                                ((0x39U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x12U
                                                                                 : 
                                                                                ((0x3aU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x80U
                                                                                 : 
                                                                                ((0x3bU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xe2U
                                                                                 : 
                                                                                ((0x3cU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xebU
                                                                                 : 
                                                                                ((0x3dU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x27U
                                                                                 : 
                                                                                ((0x3eU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xb2U
                                                                                 : 
                                                                                ((0x3fU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x75U
                                                                                 : 
                                                                                ((0x40U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 9U
                                                                                 : 
                                                                                ((0x41U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x83U
                                                                                 : 
                                                                                ((0x42U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x2cU
                                                                                 : 
                                                                                ((0x43U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x1aU
                                                                                 : 
                                                                                ((0x44U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x1bU
                                                                                 : 
                                                                                ((0x45U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x6eU
                                                                                 : 
                                                                                ((0x46U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x5aU
                                                                                 : 
                                                                                ((0x47U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xa0U
                                                                                 : 
                                                                                ((0x48U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x52U
                                                                                 : 
                                                                                ((0x49U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x3bU
                                                                                 : 
                                                                                ((0x4aU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xd6U
                                                                                 : 
                                                                                ((0x4bU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xb3U
                                                                                 : 
                                                                                ((0x4cU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x29U
                                                                                 : 
                                                                                ((0x4dU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xe3U
                                                                                 : 
                                                                                ((0x4eU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x2fU
                                                                                 : 
                                                                                ((0x4fU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x84U
                                                                                 : 
                                                                                ((0x50U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x53U
                                                                                 : 
                                                                                ((0x51U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xd1U
                                                                                 : 
                                                                                ((0x52U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0U
                                                                                 : 
                                                                                ((0x53U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xedU
                                                                                 : 
                                                                                ((0x54U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x20U
                                                                                 : 
                                                                                ((0x55U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xfcU
                                                                                 : 
                                                                                ((0x56U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xb1U
                                                                                 : 
                                                                                ((0x57U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x5bU
                                                                                 : 
                                                                                ((0x58U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x6aU
                                                                                 : 
                                                                                ((0x59U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xcbU
                                                                                 : 
                                                                                ((0x5aU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xbeU
                                                                                 : 
                                                                                ((0x5bU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x39U
                                                                                 : 
                                                                                ((0x5cU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x4aU
                                                                                 : 
                                                                                ((0x5dU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x4cU
                                                                                 : 
                                                                                ((0x5eU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x58U
                                                                                 : 
                                                                                ((0x5fU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xcfU
                                                                                 : 
                                                                                ((0x60U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xd0U
                                                                                 : 
                                                                                ((0x61U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xefU
                                                                                 : 
                                                                                ((0x62U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xaaU
                                                                                 : 
                                                                                ((0x63U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xfbU
                                                                                 : 
                                                                                ((0x64U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x43U
                                                                                 : 
                                                                                ((0x65U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x4dU
                                                                                 : 
                                                                                ((0x66U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x33U
                                                                                 : 
                                                                                ((0x67U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x85U
                                                                                 : 
                                                                                ((0x68U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x45U
                                                                                 : 
                                                                                ((0x69U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xf9U
                                                                                 : 
                                                                                ((0x6aU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 2U
                                                                                 : 
                                                                                ((0x6bU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x7fU
                                                                                 : 
                                                                                ((0x6cU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x50U
                                                                                 : 
                                                                                ((0x6dU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x3cU
                                                                                 : 
                                                                                ((0x6eU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x9fU
                                                                                 : 
                                                                                ((0x6fU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xa8U
                                                                                 : 
                                                                                ((0x70U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x51U
                                                                                 : 
                                                                                ((0x71U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xa3U
                                                                                 : 
                                                                                ((0x72U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x40U
                                                                                 : 
                                                                                ((0x73U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x8fU
                                                                                 : 
                                                                                ((0x74U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x92U
                                                                                 : 
                                                                                ((0x75U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x9dU
                                                                                 : 
                                                                                ((0x76U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x38U
                                                                                 : 
                                                                                ((0x77U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xf5U
                                                                                 : 
                                                                                ((0x78U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xbcU
                                                                                 : 
                                                                                ((0x79U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xb6U
                                                                                 : 
                                                                                ((0x7aU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xdaU
                                                                                 : 
                                                                                ((0x7bU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x21U
                                                                                 : 
                                                                                ((0x7cU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x10U
                                                                                 : 
                                                                                ((0x7dU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xffU
                                                                                 : 
                                                                                ((0x7eU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xf3U
                                                                                 : 
                                                                                ((0x7fU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xd2U
                                                                                 : 
                                                                                ((0x80U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xcdU
                                                                                 : 
                                                                                ((0x81U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xcU
                                                                                 : 
                                                                                ((0x82U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x13U
                                                                                 : 
                                                                                ((0x83U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xecU
                                                                                 : 
                                                                                ((0x84U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x5fU
                                                                                 : 
                                                                                ((0x85U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x97U
                                                                                 : 
                                                                                ((0x86U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x44U
                                                                                 : 
                                                                                ((0x87U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0x17U
                                                                                 : 
                                                                                ((0x88U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                                 ? 0xc4U
                                                                                 : __Vdeeptemp_hfded1fc4__0)))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))));
    __Vdeeptemp_h377c8bb0__0 = ((0x11U == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                 ? 0x82U : __Vdeeptemp_h4ba552b1__0);
    __Vfunc_AesCipherTop__DOT__AesSbox__13__Vfuncout 
        = ((0U == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
            ? 0x63U : ((1U == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                        ? 0x7cU : ((2U == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                    ? 0x77U : ((3U 
                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                ? 0x7bU
                                                : (
                                                   (4U 
                                                    == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                    ? 0xf2U
                                                    : 
                                                   ((5U 
                                                     == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                     ? 0x6bU
                                                     : 
                                                    ((6U 
                                                      == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                      ? 0x6fU
                                                      : 
                                                     ((7U 
                                                       == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                       ? 0xc5U
                                                       : 
                                                      ((8U 
                                                        == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                        ? 0x30U
                                                        : 
                                                       ((9U 
                                                         == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                         ? 1U
                                                         : 
                                                        ((0xaU 
                                                          == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                          ? 0x67U
                                                          : 
                                                         ((0xbU 
                                                           == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                           ? 0x2bU
                                                           : 
                                                          ((0xcU 
                                                            == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                            ? 0xfeU
                                                            : 
                                                           ((0xdU 
                                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                             ? 0xd7U
                                                             : 
                                                            ((0xeU 
                                                              == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                              ? 0xabU
                                                              : 
                                                             ((0xfU 
                                                               == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                               ? 0x76U
                                                               : 
                                                              ((0x10U 
                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__13__a))
                                                                ? 0xcaU
                                                                : __Vdeeptemp_h377c8bb0__0)))))))))))))))));
    vlSelfRef.AesCipherTop__DOT__sa31_sr = __Vfunc_AesCipherTop__DOT__AesSbox__13__Vfuncout;
    __Vfunc_AesCipherTop__DOT__AesSbox__14__a = vlSelfRef.AesCipherTop__DOT__sa31;
    __Vdeeptemp_h08d8f6a2__0 = ((0x89U == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                 ? 0xa7U : ((0x8aU 
                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                             ? 0x7eU
                                             : ((0x8bU 
                                                 == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                 ? 0x3dU
                                                 : 
                                                ((0x8cU 
                                                  == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                  ? 0x64U
                                                  : 
                                                 ((0x8dU 
                                                   == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                   ? 0x5dU
                                                   : 
                                                  ((0x8eU 
                                                    == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                    ? 0x19U
                                                    : 
                                                   ((0x8fU 
                                                     == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                     ? 0x73U
                                                     : 
                                                    ((0x90U 
                                                      == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                      ? 0x60U
                                                      : 
                                                     ((0x91U 
                                                       == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                       ? 0x81U
                                                       : 
                                                      ((0x92U 
                                                        == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                        ? 0x4fU
                                                        : 
                                                       ((0x93U 
                                                         == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                         ? 0xdcU
                                                         : 
                                                        ((0x94U 
                                                          == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                          ? 0x22U
                                                          : 
                                                         ((0x95U 
                                                           == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                           ? 0x2aU
                                                           : 
                                                          ((0x96U 
                                                            == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                            ? 0x90U
                                                            : 
                                                           ((0x97U 
                                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                             ? 0x88U
                                                             : 
                                                            ((0x98U 
                                                              == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                              ? 0x46U
                                                              : 
                                                             ((0x99U 
                                                               == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                               ? 0xeeU
                                                               : 
                                                              ((0x9aU 
                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                ? 0xb8U
                                                                : 
                                                               ((0x9bU 
                                                                 == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                 ? 0x14U
                                                                 : 
                                                                ((0x9cU 
                                                                  == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                  ? 0xdeU
                                                                  : 
                                                                 ((0x9dU 
                                                                   == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                   ? 0x5eU
                                                                   : 
                                                                  ((0x9eU 
                                                                    == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                    ? 0xbU
                                                                    : 
                                                                   ((0x9fU 
                                                                     == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                     ? 0xdbU
                                                                     : 
                                                                    ((0xa0U 
                                                                      == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                      ? 0xe0U
                                                                      : 
                                                                     ((0xa1U 
                                                                       == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                       ? 0x32U
                                                                       : 
                                                                      ((0xa2U 
                                                                        == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                        ? 0x3aU
                                                                        : 
                                                                       ((0xa3U 
                                                                         == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                         ? 0xaU
                                                                         : 
                                                                        ((0xa4U 
                                                                          == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                          ? 0x49U
                                                                          : 
                                                                         ((0xa5U 
                                                                           == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                           ? 6U
                                                                           : 
                                                                          ((0xa6U 
                                                                            == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                            ? 0x24U
                                                                            : 
                                                                           ((0xa7U 
                                                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                             ? 0x5cU
                                                                             : 
                                                                            ((0xa8U 
                                                                              == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                              ? 0xc2U
                                                                              : 
                                                                             ((0xa9U 
                                                                               == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                               ? 0xd3U
                                                                               : 
                                                                              ((0xaaU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                ? 0xacU
                                                                                : 
                                                                               ((0xabU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x62U
                                                                                 : 
                                                                                ((0xacU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x91U
                                                                                 : 
                                                                                ((0xadU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x95U
                                                                                 : 
                                                                                ((0xaeU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xe4U
                                                                                 : 
                                                                                ((0xafU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x79U
                                                                                 : 
                                                                                ((0xb0U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xe7U
                                                                                 : 
                                                                                ((0xb1U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xc8U
                                                                                 : 
                                                                                ((0xb2U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x37U
                                                                                 : 
                                                                                ((0xb3U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x6dU
                                                                                 : 
                                                                                ((0xb4U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x8dU
                                                                                 : 
                                                                                ((0xb5U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xd5U
                                                                                 : 
                                                                                ((0xb6U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x4eU
                                                                                 : 
                                                                                ((0xb7U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xa9U
                                                                                 : 
                                                                                ((0xb8U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x6cU
                                                                                 : 
                                                                                ((0xb9U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x56U
                                                                                 : 
                                                                                ((0xbaU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xf4U
                                                                                 : 
                                                                                ((0xbbU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xeaU
                                                                                 : 
                                                                                ((0xbcU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x65U
                                                                                 : 
                                                                                ((0xbdU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x7aU
                                                                                 : 
                                                                                ((0xbeU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xaeU
                                                                                 : 
                                                                                ((0xbfU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 8U
                                                                                 : 
                                                                                ((0xc0U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xbaU
                                                                                 : 
                                                                                ((0xc1U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x78U
                                                                                 : 
                                                                                ((0xc2U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x25U
                                                                                 : 
                                                                                ((0xc3U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x2eU
                                                                                 : 
                                                                                ((0xc4U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x1cU
                                                                                 : 
                                                                                ((0xc5U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xa6U
                                                                                 : 
                                                                                ((0xc6U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xb4U
                                                                                 : 
                                                                                ((0xc7U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xc6U
                                                                                 : 
                                                                                ((0xc8U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xe8U
                                                                                 : 
                                                                                ((0xc9U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xddU
                                                                                 : 
                                                                                ((0xcaU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x74U
                                                                                 : 
                                                                                ((0xcbU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x1fU
                                                                                 : 
                                                                                ((0xccU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x4bU
                                                                                 : 
                                                                                ((0xcdU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xbdU
                                                                                 : 
                                                                                ((0xceU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x8bU
                                                                                 : 
                                                                                ((0xcfU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x8aU
                                                                                 : 
                                                                                ((0xd0U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x70U
                                                                                 : 
                                                                                ((0xd1U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x3eU
                                                                                 : 
                                                                                ((0xd2U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xb5U
                                                                                 : 
                                                                                ((0xd3U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x66U
                                                                                 : 
                                                                                ((0xd4U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x48U
                                                                                 : 
                                                                                ((0xd5U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 3U
                                                                                 : 
                                                                                ((0xd6U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xf6U
                                                                                 : 
                                                                                ((0xd7U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xeU
                                                                                 : 
                                                                                ((0xd8U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x61U
                                                                                 : 
                                                                                ((0xd9U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x35U
                                                                                 : 
                                                                                ((0xdaU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x57U
                                                                                 : 
                                                                                ((0xdbU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xb9U
                                                                                 : 
                                                                                ((0xdcU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x86U
                                                                                 : 
                                                                                ((0xddU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xc1U
                                                                                 : 
                                                                                ((0xdeU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x1dU
                                                                                 : 
                                                                                ((0xdfU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x9eU
                                                                                 : 
                                                                                ((0xe0U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xe1U
                                                                                 : 
                                                                                ((0xe1U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xf8U
                                                                                 : 
                                                                                ((0xe2U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x98U
                                                                                 : 
                                                                                ((0xe3U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x11U
                                                                                 : 
                                                                                ((0xe4U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x69U
                                                                                 : 
                                                                                ((0xe5U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xd9U
                                                                                 : 
                                                                                ((0xe6U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x8eU
                                                                                 : 
                                                                                ((0xe7U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x94U
                                                                                 : 
                                                                                ((0xe8U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x9bU
                                                                                 : 
                                                                                ((0xe9U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x1eU
                                                                                 : 
                                                                                ((0xeaU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x87U
                                                                                 : 
                                                                                ((0xebU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xe9U
                                                                                 : 
                                                                                ((0xecU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xceU
                                                                                 : 
                                                                                ((0xedU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x55U
                                                                                 : 
                                                                                ((0xeeU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x28U
                                                                                 : 
                                                                                ((0xefU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xdfU
                                                                                 : 
                                                                                ((0xf0U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x8cU
                                                                                 : 
                                                                                ((0xf1U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xa1U
                                                                                 : 
                                                                                ((0xf2U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x89U
                                                                                 : 
                                                                                ((0xf3U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xdU
                                                                                 : 
                                                                                ((0xf4U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xbfU
                                                                                 : 
                                                                                ((0xf5U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xe6U
                                                                                 : 
                                                                                ((0xf6U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x42U
                                                                                 : 
                                                                                ((0xf7U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x68U
                                                                                 : 
                                                                                ((0xf8U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x41U
                                                                                 : 
                                                                                ((0xf9U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x99U
                                                                                 : 
                                                                                ((0xfaU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x2dU
                                                                                 : 
                                                                                ((0xfbU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xfU
                                                                                 : 
                                                                                ((0xfcU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xb0U
                                                                                 : 
                                                                                ((0xfdU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x54U
                                                                                 : 
                                                                                ((0xfeU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xbbU
                                                                                 : 
                                                                                ((0xffU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x16U
                                                                                 : 0U)))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))));
    __Vdeeptemp_hafb8d4b2__0 = ((0x12U == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                 ? 0xc9U : ((0x13U 
                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                             ? 0x7dU
                                             : ((0x14U 
                                                 == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                 ? 0xfaU
                                                 : 
                                                ((0x15U 
                                                  == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                  ? 0x59U
                                                  : 
                                                 ((0x16U 
                                                   == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                   ? 0x47U
                                                   : 
                                                  ((0x17U 
                                                    == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                    ? 0xf0U
                                                    : 
                                                   ((0x18U 
                                                     == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                     ? 0xadU
                                                     : 
                                                    ((0x19U 
                                                      == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                      ? 0xd4U
                                                      : 
                                                     ((0x1aU 
                                                       == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                       ? 0xa2U
                                                       : 
                                                      ((0x1bU 
                                                        == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                        ? 0xafU
                                                        : 
                                                       ((0x1cU 
                                                         == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                         ? 0x9cU
                                                         : 
                                                        ((0x1dU 
                                                          == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                          ? 0xa4U
                                                          : 
                                                         ((0x1eU 
                                                           == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                           ? 0x72U
                                                           : 
                                                          ((0x1fU 
                                                            == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                            ? 0xc0U
                                                            : 
                                                           ((0x20U 
                                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                             ? 0xb7U
                                                             : 
                                                            ((0x21U 
                                                              == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                              ? 0xfdU
                                                              : 
                                                             ((0x22U 
                                                               == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                               ? 0x93U
                                                               : 
                                                              ((0x23U 
                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                ? 0x26U
                                                                : 
                                                               ((0x24U 
                                                                 == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                 ? 0x36U
                                                                 : 
                                                                ((0x25U 
                                                                  == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                  ? 0x3fU
                                                                  : 
                                                                 ((0x26U 
                                                                   == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                   ? 0xf7U
                                                                   : 
                                                                  ((0x27U 
                                                                    == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                    ? 0xccU
                                                                    : 
                                                                   ((0x28U 
                                                                     == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                     ? 0x34U
                                                                     : 
                                                                    ((0x29U 
                                                                      == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                      ? 0xa5U
                                                                      : 
                                                                     ((0x2aU 
                                                                       == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                       ? 0xe5U
                                                                       : 
                                                                      ((0x2bU 
                                                                        == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                        ? 0xf1U
                                                                        : 
                                                                       ((0x2cU 
                                                                         == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                         ? 0x71U
                                                                         : 
                                                                        ((0x2dU 
                                                                          == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                          ? 0xd8U
                                                                          : 
                                                                         ((0x2eU 
                                                                           == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                           ? 0x31U
                                                                           : 
                                                                          ((0x2fU 
                                                                            == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                            ? 0x15U
                                                                            : 
                                                                           ((0x30U 
                                                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                             ? 4U
                                                                             : 
                                                                            ((0x31U 
                                                                              == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                              ? 0xc7U
                                                                              : 
                                                                             ((0x32U 
                                                                               == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                               ? 0x23U
                                                                               : 
                                                                              ((0x33U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                ? 0xc3U
                                                                                : 
                                                                               ((0x34U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x18U
                                                                                 : 
                                                                                ((0x35U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x96U
                                                                                 : 
                                                                                ((0x36U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 5U
                                                                                 : 
                                                                                ((0x37U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x9aU
                                                                                 : 
                                                                                ((0x38U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 7U
                                                                                 : 
                                                                                ((0x39U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x12U
                                                                                 : 
                                                                                ((0x3aU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x80U
                                                                                 : 
                                                                                ((0x3bU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xe2U
                                                                                 : 
                                                                                ((0x3cU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xebU
                                                                                 : 
                                                                                ((0x3dU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x27U
                                                                                 : 
                                                                                ((0x3eU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xb2U
                                                                                 : 
                                                                                ((0x3fU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x75U
                                                                                 : 
                                                                                ((0x40U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 9U
                                                                                 : 
                                                                                ((0x41U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x83U
                                                                                 : 
                                                                                ((0x42U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x2cU
                                                                                 : 
                                                                                ((0x43U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x1aU
                                                                                 : 
                                                                                ((0x44U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x1bU
                                                                                 : 
                                                                                ((0x45U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x6eU
                                                                                 : 
                                                                                ((0x46U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x5aU
                                                                                 : 
                                                                                ((0x47U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xa0U
                                                                                 : 
                                                                                ((0x48U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x52U
                                                                                 : 
                                                                                ((0x49U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x3bU
                                                                                 : 
                                                                                ((0x4aU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xd6U
                                                                                 : 
                                                                                ((0x4bU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xb3U
                                                                                 : 
                                                                                ((0x4cU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x29U
                                                                                 : 
                                                                                ((0x4dU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xe3U
                                                                                 : 
                                                                                ((0x4eU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x2fU
                                                                                 : 
                                                                                ((0x4fU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x84U
                                                                                 : 
                                                                                ((0x50U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x53U
                                                                                 : 
                                                                                ((0x51U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xd1U
                                                                                 : 
                                                                                ((0x52U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0U
                                                                                 : 
                                                                                ((0x53U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xedU
                                                                                 : 
                                                                                ((0x54U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x20U
                                                                                 : 
                                                                                ((0x55U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xfcU
                                                                                 : 
                                                                                ((0x56U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xb1U
                                                                                 : 
                                                                                ((0x57U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x5bU
                                                                                 : 
                                                                                ((0x58U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x6aU
                                                                                 : 
                                                                                ((0x59U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xcbU
                                                                                 : 
                                                                                ((0x5aU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xbeU
                                                                                 : 
                                                                                ((0x5bU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x39U
                                                                                 : 
                                                                                ((0x5cU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x4aU
                                                                                 : 
                                                                                ((0x5dU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x4cU
                                                                                 : 
                                                                                ((0x5eU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x58U
                                                                                 : 
                                                                                ((0x5fU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xcfU
                                                                                 : 
                                                                                ((0x60U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xd0U
                                                                                 : 
                                                                                ((0x61U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xefU
                                                                                 : 
                                                                                ((0x62U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xaaU
                                                                                 : 
                                                                                ((0x63U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xfbU
                                                                                 : 
                                                                                ((0x64U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x43U
                                                                                 : 
                                                                                ((0x65U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x4dU
                                                                                 : 
                                                                                ((0x66U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x33U
                                                                                 : 
                                                                                ((0x67U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x85U
                                                                                 : 
                                                                                ((0x68U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x45U
                                                                                 : 
                                                                                ((0x69U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xf9U
                                                                                 : 
                                                                                ((0x6aU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 2U
                                                                                 : 
                                                                                ((0x6bU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x7fU
                                                                                 : 
                                                                                ((0x6cU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x50U
                                                                                 : 
                                                                                ((0x6dU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x3cU
                                                                                 : 
                                                                                ((0x6eU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x9fU
                                                                                 : 
                                                                                ((0x6fU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xa8U
                                                                                 : 
                                                                                ((0x70U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x51U
                                                                                 : 
                                                                                ((0x71U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xa3U
                                                                                 : 
                                                                                ((0x72U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x40U
                                                                                 : 
                                                                                ((0x73U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x8fU
                                                                                 : 
                                                                                ((0x74U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x92U
                                                                                 : 
                                                                                ((0x75U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x9dU
                                                                                 : 
                                                                                ((0x76U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x38U
                                                                                 : 
                                                                                ((0x77U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xf5U
                                                                                 : 
                                                                                ((0x78U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xbcU
                                                                                 : 
                                                                                ((0x79U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xb6U
                                                                                 : 
                                                                                ((0x7aU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xdaU
                                                                                 : 
                                                                                ((0x7bU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x21U
                                                                                 : 
                                                                                ((0x7cU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x10U
                                                                                 : 
                                                                                ((0x7dU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xffU
                                                                                 : 
                                                                                ((0x7eU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xf3U
                                                                                 : 
                                                                                ((0x7fU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xd2U
                                                                                 : 
                                                                                ((0x80U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xcdU
                                                                                 : 
                                                                                ((0x81U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xcU
                                                                                 : 
                                                                                ((0x82U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x13U
                                                                                 : 
                                                                                ((0x83U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xecU
                                                                                 : 
                                                                                ((0x84U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x5fU
                                                                                 : 
                                                                                ((0x85U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x97U
                                                                                 : 
                                                                                ((0x86U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x44U
                                                                                 : 
                                                                                ((0x87U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0x17U
                                                                                 : 
                                                                                ((0x88U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                                 ? 0xc4U
                                                                                 : __Vdeeptemp_h08d8f6a2__0)))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))));
    __Vdeeptemp_h24ee4228__0 = ((0x11U == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                 ? 0x82U : __Vdeeptemp_hafb8d4b2__0);
    __Vfunc_AesCipherTop__DOT__AesSbox__14__Vfuncout 
        = ((0U == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
            ? 0x63U : ((1U == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                        ? 0x7cU : ((2U == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                    ? 0x77U : ((3U 
                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                ? 0x7bU
                                                : (
                                                   (4U 
                                                    == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                    ? 0xf2U
                                                    : 
                                                   ((5U 
                                                     == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                     ? 0x6bU
                                                     : 
                                                    ((6U 
                                                      == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                      ? 0x6fU
                                                      : 
                                                     ((7U 
                                                       == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                       ? 0xc5U
                                                       : 
                                                      ((8U 
                                                        == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                        ? 0x30U
                                                        : 
                                                       ((9U 
                                                         == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                         ? 1U
                                                         : 
                                                        ((0xaU 
                                                          == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                          ? 0x67U
                                                          : 
                                                         ((0xbU 
                                                           == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                           ? 0x2bU
                                                           : 
                                                          ((0xcU 
                                                            == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                            ? 0xfeU
                                                            : 
                                                           ((0xdU 
                                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                             ? 0xd7U
                                                             : 
                                                            ((0xeU 
                                                              == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                              ? 0xabU
                                                              : 
                                                             ((0xfU 
                                                               == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                               ? 0x76U
                                                               : 
                                                              ((0x10U 
                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__14__a))
                                                                ? 0xcaU
                                                                : __Vdeeptemp_h24ee4228__0)))))))))))))))));
    vlSelfRef.AesCipherTop__DOT__sa32_sr = __Vfunc_AesCipherTop__DOT__AesSbox__14__Vfuncout;
    __Vfunc_AesCipherTop__DOT__AesSbox__15__a = vlSelfRef.AesCipherTop__DOT__sa32;
    __Vdeeptemp_h362e9fff__0 = ((0x89U == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                 ? 0xa7U : ((0x8aU 
                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                             ? 0x7eU
                                             : ((0x8bU 
                                                 == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                 ? 0x3dU
                                                 : 
                                                ((0x8cU 
                                                  == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                  ? 0x64U
                                                  : 
                                                 ((0x8dU 
                                                   == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                   ? 0x5dU
                                                   : 
                                                  ((0x8eU 
                                                    == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                    ? 0x19U
                                                    : 
                                                   ((0x8fU 
                                                     == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                     ? 0x73U
                                                     : 
                                                    ((0x90U 
                                                      == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                      ? 0x60U
                                                      : 
                                                     ((0x91U 
                                                       == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                       ? 0x81U
                                                       : 
                                                      ((0x92U 
                                                        == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                        ? 0x4fU
                                                        : 
                                                       ((0x93U 
                                                         == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                         ? 0xdcU
                                                         : 
                                                        ((0x94U 
                                                          == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                          ? 0x22U
                                                          : 
                                                         ((0x95U 
                                                           == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                           ? 0x2aU
                                                           : 
                                                          ((0x96U 
                                                            == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                            ? 0x90U
                                                            : 
                                                           ((0x97U 
                                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                             ? 0x88U
                                                             : 
                                                            ((0x98U 
                                                              == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                              ? 0x46U
                                                              : 
                                                             ((0x99U 
                                                               == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                               ? 0xeeU
                                                               : 
                                                              ((0x9aU 
                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                ? 0xb8U
                                                                : 
                                                               ((0x9bU 
                                                                 == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                 ? 0x14U
                                                                 : 
                                                                ((0x9cU 
                                                                  == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                  ? 0xdeU
                                                                  : 
                                                                 ((0x9dU 
                                                                   == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                   ? 0x5eU
                                                                   : 
                                                                  ((0x9eU 
                                                                    == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                    ? 0xbU
                                                                    : 
                                                                   ((0x9fU 
                                                                     == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                     ? 0xdbU
                                                                     : 
                                                                    ((0xa0U 
                                                                      == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                      ? 0xe0U
                                                                      : 
                                                                     ((0xa1U 
                                                                       == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                       ? 0x32U
                                                                       : 
                                                                      ((0xa2U 
                                                                        == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                        ? 0x3aU
                                                                        : 
                                                                       ((0xa3U 
                                                                         == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                         ? 0xaU
                                                                         : 
                                                                        ((0xa4U 
                                                                          == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                          ? 0x49U
                                                                          : 
                                                                         ((0xa5U 
                                                                           == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                           ? 6U
                                                                           : 
                                                                          ((0xa6U 
                                                                            == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                            ? 0x24U
                                                                            : 
                                                                           ((0xa7U 
                                                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                             ? 0x5cU
                                                                             : 
                                                                            ((0xa8U 
                                                                              == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                              ? 0xc2U
                                                                              : 
                                                                             ((0xa9U 
                                                                               == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                               ? 0xd3U
                                                                               : 
                                                                              ((0xaaU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                ? 0xacU
                                                                                : 
                                                                               ((0xabU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x62U
                                                                                 : 
                                                                                ((0xacU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x91U
                                                                                 : 
                                                                                ((0xadU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x95U
                                                                                 : 
                                                                                ((0xaeU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xe4U
                                                                                 : 
                                                                                ((0xafU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x79U
                                                                                 : 
                                                                                ((0xb0U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xe7U
                                                                                 : 
                                                                                ((0xb1U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xc8U
                                                                                 : 
                                                                                ((0xb2U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x37U
                                                                                 : 
                                                                                ((0xb3U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x6dU
                                                                                 : 
                                                                                ((0xb4U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x8dU
                                                                                 : 
                                                                                ((0xb5U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xd5U
                                                                                 : 
                                                                                ((0xb6U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x4eU
                                                                                 : 
                                                                                ((0xb7U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xa9U
                                                                                 : 
                                                                                ((0xb8U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x6cU
                                                                                 : 
                                                                                ((0xb9U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x56U
                                                                                 : 
                                                                                ((0xbaU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xf4U
                                                                                 : 
                                                                                ((0xbbU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xeaU
                                                                                 : 
                                                                                ((0xbcU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x65U
                                                                                 : 
                                                                                ((0xbdU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x7aU
                                                                                 : 
                                                                                ((0xbeU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xaeU
                                                                                 : 
                                                                                ((0xbfU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 8U
                                                                                 : 
                                                                                ((0xc0U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xbaU
                                                                                 : 
                                                                                ((0xc1U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x78U
                                                                                 : 
                                                                                ((0xc2U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x25U
                                                                                 : 
                                                                                ((0xc3U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x2eU
                                                                                 : 
                                                                                ((0xc4U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x1cU
                                                                                 : 
                                                                                ((0xc5U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xa6U
                                                                                 : 
                                                                                ((0xc6U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xb4U
                                                                                 : 
                                                                                ((0xc7U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xc6U
                                                                                 : 
                                                                                ((0xc8U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xe8U
                                                                                 : 
                                                                                ((0xc9U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xddU
                                                                                 : 
                                                                                ((0xcaU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x74U
                                                                                 : 
                                                                                ((0xcbU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x1fU
                                                                                 : 
                                                                                ((0xccU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x4bU
                                                                                 : 
                                                                                ((0xcdU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xbdU
                                                                                 : 
                                                                                ((0xceU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x8bU
                                                                                 : 
                                                                                ((0xcfU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x8aU
                                                                                 : 
                                                                                ((0xd0U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x70U
                                                                                 : 
                                                                                ((0xd1U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x3eU
                                                                                 : 
                                                                                ((0xd2U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xb5U
                                                                                 : 
                                                                                ((0xd3U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x66U
                                                                                 : 
                                                                                ((0xd4U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x48U
                                                                                 : 
                                                                                ((0xd5U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 3U
                                                                                 : 
                                                                                ((0xd6U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xf6U
                                                                                 : 
                                                                                ((0xd7U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xeU
                                                                                 : 
                                                                                ((0xd8U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x61U
                                                                                 : 
                                                                                ((0xd9U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x35U
                                                                                 : 
                                                                                ((0xdaU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x57U
                                                                                 : 
                                                                                ((0xdbU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xb9U
                                                                                 : 
                                                                                ((0xdcU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x86U
                                                                                 : 
                                                                                ((0xddU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xc1U
                                                                                 : 
                                                                                ((0xdeU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x1dU
                                                                                 : 
                                                                                ((0xdfU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x9eU
                                                                                 : 
                                                                                ((0xe0U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xe1U
                                                                                 : 
                                                                                ((0xe1U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xf8U
                                                                                 : 
                                                                                ((0xe2U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x98U
                                                                                 : 
                                                                                ((0xe3U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x11U
                                                                                 : 
                                                                                ((0xe4U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x69U
                                                                                 : 
                                                                                ((0xe5U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xd9U
                                                                                 : 
                                                                                ((0xe6U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x8eU
                                                                                 : 
                                                                                ((0xe7U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x94U
                                                                                 : 
                                                                                ((0xe8U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x9bU
                                                                                 : 
                                                                                ((0xe9U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x1eU
                                                                                 : 
                                                                                ((0xeaU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x87U
                                                                                 : 
                                                                                ((0xebU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xe9U
                                                                                 : 
                                                                                ((0xecU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xceU
                                                                                 : 
                                                                                ((0xedU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x55U
                                                                                 : 
                                                                                ((0xeeU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x28U
                                                                                 : 
                                                                                ((0xefU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xdfU
                                                                                 : 
                                                                                ((0xf0U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x8cU
                                                                                 : 
                                                                                ((0xf1U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xa1U
                                                                                 : 
                                                                                ((0xf2U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x89U
                                                                                 : 
                                                                                ((0xf3U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xdU
                                                                                 : 
                                                                                ((0xf4U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xbfU
                                                                                 : 
                                                                                ((0xf5U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xe6U
                                                                                 : 
                                                                                ((0xf6U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x42U
                                                                                 : 
                                                                                ((0xf7U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x68U
                                                                                 : 
                                                                                ((0xf8U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x41U
                                                                                 : 
                                                                                ((0xf9U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x99U
                                                                                 : 
                                                                                ((0xfaU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x2dU
                                                                                 : 
                                                                                ((0xfbU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xfU
                                                                                 : 
                                                                                ((0xfcU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xb0U
                                                                                 : 
                                                                                ((0xfdU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x54U
                                                                                 : 
                                                                                ((0xfeU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xbbU
                                                                                 : 
                                                                                ((0xffU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x16U
                                                                                 : 0U)))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))));
    __Vdeeptemp_h1fde1343__0 = ((0x12U == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                 ? 0xc9U : ((0x13U 
                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                             ? 0x7dU
                                             : ((0x14U 
                                                 == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                 ? 0xfaU
                                                 : 
                                                ((0x15U 
                                                  == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                  ? 0x59U
                                                  : 
                                                 ((0x16U 
                                                   == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                   ? 0x47U
                                                   : 
                                                  ((0x17U 
                                                    == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                    ? 0xf0U
                                                    : 
                                                   ((0x18U 
                                                     == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                     ? 0xadU
                                                     : 
                                                    ((0x19U 
                                                      == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                      ? 0xd4U
                                                      : 
                                                     ((0x1aU 
                                                       == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                       ? 0xa2U
                                                       : 
                                                      ((0x1bU 
                                                        == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                        ? 0xafU
                                                        : 
                                                       ((0x1cU 
                                                         == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                         ? 0x9cU
                                                         : 
                                                        ((0x1dU 
                                                          == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                          ? 0xa4U
                                                          : 
                                                         ((0x1eU 
                                                           == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                           ? 0x72U
                                                           : 
                                                          ((0x1fU 
                                                            == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                            ? 0xc0U
                                                            : 
                                                           ((0x20U 
                                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                             ? 0xb7U
                                                             : 
                                                            ((0x21U 
                                                              == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                              ? 0xfdU
                                                              : 
                                                             ((0x22U 
                                                               == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                               ? 0x93U
                                                               : 
                                                              ((0x23U 
                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                ? 0x26U
                                                                : 
                                                               ((0x24U 
                                                                 == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                 ? 0x36U
                                                                 : 
                                                                ((0x25U 
                                                                  == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                  ? 0x3fU
                                                                  : 
                                                                 ((0x26U 
                                                                   == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                   ? 0xf7U
                                                                   : 
                                                                  ((0x27U 
                                                                    == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                    ? 0xccU
                                                                    : 
                                                                   ((0x28U 
                                                                     == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                     ? 0x34U
                                                                     : 
                                                                    ((0x29U 
                                                                      == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                      ? 0xa5U
                                                                      : 
                                                                     ((0x2aU 
                                                                       == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                       ? 0xe5U
                                                                       : 
                                                                      ((0x2bU 
                                                                        == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                        ? 0xf1U
                                                                        : 
                                                                       ((0x2cU 
                                                                         == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                         ? 0x71U
                                                                         : 
                                                                        ((0x2dU 
                                                                          == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                          ? 0xd8U
                                                                          : 
                                                                         ((0x2eU 
                                                                           == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                           ? 0x31U
                                                                           : 
                                                                          ((0x2fU 
                                                                            == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                            ? 0x15U
                                                                            : 
                                                                           ((0x30U 
                                                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                             ? 4U
                                                                             : 
                                                                            ((0x31U 
                                                                              == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                              ? 0xc7U
                                                                              : 
                                                                             ((0x32U 
                                                                               == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                               ? 0x23U
                                                                               : 
                                                                              ((0x33U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                ? 0xc3U
                                                                                : 
                                                                               ((0x34U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x18U
                                                                                 : 
                                                                                ((0x35U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x96U
                                                                                 : 
                                                                                ((0x36U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 5U
                                                                                 : 
                                                                                ((0x37U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x9aU
                                                                                 : 
                                                                                ((0x38U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 7U
                                                                                 : 
                                                                                ((0x39U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x12U
                                                                                 : 
                                                                                ((0x3aU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x80U
                                                                                 : 
                                                                                ((0x3bU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xe2U
                                                                                 : 
                                                                                ((0x3cU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xebU
                                                                                 : 
                                                                                ((0x3dU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x27U
                                                                                 : 
                                                                                ((0x3eU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xb2U
                                                                                 : 
                                                                                ((0x3fU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x75U
                                                                                 : 
                                                                                ((0x40U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 9U
                                                                                 : 
                                                                                ((0x41U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x83U
                                                                                 : 
                                                                                ((0x42U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x2cU
                                                                                 : 
                                                                                ((0x43U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x1aU
                                                                                 : 
                                                                                ((0x44U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x1bU
                                                                                 : 
                                                                                ((0x45U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x6eU
                                                                                 : 
                                                                                ((0x46U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x5aU
                                                                                 : 
                                                                                ((0x47U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xa0U
                                                                                 : 
                                                                                ((0x48U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x52U
                                                                                 : 
                                                                                ((0x49U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x3bU
                                                                                 : 
                                                                                ((0x4aU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xd6U
                                                                                 : 
                                                                                ((0x4bU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xb3U
                                                                                 : 
                                                                                ((0x4cU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x29U
                                                                                 : 
                                                                                ((0x4dU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xe3U
                                                                                 : 
                                                                                ((0x4eU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x2fU
                                                                                 : 
                                                                                ((0x4fU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x84U
                                                                                 : 
                                                                                ((0x50U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x53U
                                                                                 : 
                                                                                ((0x51U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xd1U
                                                                                 : 
                                                                                ((0x52U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0U
                                                                                 : 
                                                                                ((0x53U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xedU
                                                                                 : 
                                                                                ((0x54U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x20U
                                                                                 : 
                                                                                ((0x55U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xfcU
                                                                                 : 
                                                                                ((0x56U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xb1U
                                                                                 : 
                                                                                ((0x57U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x5bU
                                                                                 : 
                                                                                ((0x58U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x6aU
                                                                                 : 
                                                                                ((0x59U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xcbU
                                                                                 : 
                                                                                ((0x5aU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xbeU
                                                                                 : 
                                                                                ((0x5bU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x39U
                                                                                 : 
                                                                                ((0x5cU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x4aU
                                                                                 : 
                                                                                ((0x5dU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x4cU
                                                                                 : 
                                                                                ((0x5eU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x58U
                                                                                 : 
                                                                                ((0x5fU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xcfU
                                                                                 : 
                                                                                ((0x60U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xd0U
                                                                                 : 
                                                                                ((0x61U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xefU
                                                                                 : 
                                                                                ((0x62U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xaaU
                                                                                 : 
                                                                                ((0x63U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xfbU
                                                                                 : 
                                                                                ((0x64U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x43U
                                                                                 : 
                                                                                ((0x65U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x4dU
                                                                                 : 
                                                                                ((0x66U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x33U
                                                                                 : 
                                                                                ((0x67U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x85U
                                                                                 : 
                                                                                ((0x68U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x45U
                                                                                 : 
                                                                                ((0x69U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xf9U
                                                                                 : 
                                                                                ((0x6aU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 2U
                                                                                 : 
                                                                                ((0x6bU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x7fU
                                                                                 : 
                                                                                ((0x6cU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x50U
                                                                                 : 
                                                                                ((0x6dU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x3cU
                                                                                 : 
                                                                                ((0x6eU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x9fU
                                                                                 : 
                                                                                ((0x6fU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xa8U
                                                                                 : 
                                                                                ((0x70U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x51U
                                                                                 : 
                                                                                ((0x71U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xa3U
                                                                                 : 
                                                                                ((0x72U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x40U
                                                                                 : 
                                                                                ((0x73U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x8fU
                                                                                 : 
                                                                                ((0x74U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x92U
                                                                                 : 
                                                                                ((0x75U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x9dU
                                                                                 : 
                                                                                ((0x76U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x38U
                                                                                 : 
                                                                                ((0x77U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xf5U
                                                                                 : 
                                                                                ((0x78U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xbcU
                                                                                 : 
                                                                                ((0x79U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xb6U
                                                                                 : 
                                                                                ((0x7aU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xdaU
                                                                                 : 
                                                                                ((0x7bU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x21U
                                                                                 : 
                                                                                ((0x7cU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x10U
                                                                                 : 
                                                                                ((0x7dU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xffU
                                                                                 : 
                                                                                ((0x7eU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xf3U
                                                                                 : 
                                                                                ((0x7fU 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xd2U
                                                                                 : 
                                                                                ((0x80U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xcdU
                                                                                 : 
                                                                                ((0x81U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xcU
                                                                                 : 
                                                                                ((0x82U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x13U
                                                                                 : 
                                                                                ((0x83U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xecU
                                                                                 : 
                                                                                ((0x84U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x5fU
                                                                                 : 
                                                                                ((0x85U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x97U
                                                                                 : 
                                                                                ((0x86U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x44U
                                                                                 : 
                                                                                ((0x87U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0x17U
                                                                                 : 
                                                                                ((0x88U 
                                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                                 ? 0xc4U
                                                                                 : __Vdeeptemp_h362e9fff__0)))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))));
    __Vdeeptemp_hbcda1274__0 = ((0x11U == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                 ? 0x82U : __Vdeeptemp_h1fde1343__0);
    vlSelfRef.__Vfunc_AesCipherTop__DOT__AesSbox__15__Vfuncout 
        = ((0U == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
            ? 0x63U : ((1U == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                        ? 0x7cU : ((2U == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                    ? 0x77U : ((3U 
                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                ? 0x7bU
                                                : (
                                                   (4U 
                                                    == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                    ? 0xf2U
                                                    : 
                                                   ((5U 
                                                     == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                     ? 0x6bU
                                                     : 
                                                    ((6U 
                                                      == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                      ? 0x6fU
                                                      : 
                                                     ((7U 
                                                       == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                       ? 0xc5U
                                                       : 
                                                      ((8U 
                                                        == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                        ? 0x30U
                                                        : 
                                                       ((9U 
                                                         == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                         ? 1U
                                                         : 
                                                        ((0xaU 
                                                          == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                          ? 0x67U
                                                          : 
                                                         ((0xbU 
                                                           == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                           ? 0x2bU
                                                           : 
                                                          ((0xcU 
                                                            == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                            ? 0xfeU
                                                            : 
                                                           ((0xdU 
                                                             == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                             ? 0xd7U
                                                             : 
                                                            ((0xeU 
                                                              == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                              ? 0xabU
                                                              : 
                                                             ((0xfU 
                                                               == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                               ? 0x76U
                                                               : 
                                                              ((0x10U 
                                                                == (IData)(__Vfunc_AesCipherTop__DOT__AesSbox__15__a))
                                                                ? 0xcaU
                                                                : __Vdeeptemp_hbcda1274__0)))))))))))))))));
    AesCipherTop__DOT__sa33_sr = vlSelfRef.__Vfunc_AesCipherTop__DOT__AesSbox__15__Vfuncout;
    vlSelfRef.AesCipherTop__DOT__key_exp__DOT__nw0 
        = (vlSelfRef.AesCipherTop__DOT__key_exp__DOT__w0 
           ^ (vlSelfRef.AesCipherTop__DOT__key_exp__DOT__subword 
              ^ ((0U == (IData)(vlSelfRef.AesCipherTop__DOT__key_exp__DOT__rcnt))
                  ? 0x1000000U : ((1U == (IData)(vlSelfRef.AesCipherTop__DOT__key_exp__DOT__rcnt))
                                   ? 0x2000000U : (
                                                   (2U 
                                                    == (IData)(vlSelfRef.AesCipherTop__DOT__key_exp__DOT__rcnt))
                                                    ? 0x4000000U
                                                    : 
                                                   ((3U 
                                                     == (IData)(vlSelfRef.AesCipherTop__DOT__key_exp__DOT__rcnt))
                                                     ? 0x8000000U
                                                     : 
                                                    ((4U 
                                                      == (IData)(vlSelfRef.AesCipherTop__DOT__key_exp__DOT__rcnt))
                                                      ? 0x10000000U
                                                      : 
                                                     ((5U 
                                                       == (IData)(vlSelfRef.AesCipherTop__DOT__key_exp__DOT__rcnt))
                                                       ? 0x20000000U
                                                       : 
                                                      ((6U 
                                                        == (IData)(vlSelfRef.AesCipherTop__DOT__key_exp__DOT__rcnt))
                                                        ? 0x40000000U
                                                        : 
                                                       ((7U 
                                                         == (IData)(vlSelfRef.AesCipherTop__DOT__key_exp__DOT__rcnt))
                                                         ? 0x80000000U
                                                         : 
                                                        ((8U 
                                                          == (IData)(vlSelfRef.AesCipherTop__DOT__key_exp__DOT__rcnt))
                                                          ? 0x1b000000U
                                                          : 
                                                         ((9U 
                                                           == (IData)(vlSelfRef.AesCipherTop__DOT__key_exp__DOT__rcnt))
                                                           ? 0x36000000U
                                                           : 0U))))))))))));
    vlSelfRef.AesCipherTop__DOT__sa00_fark = (0xffU 
                                              & ((IData)(vlSelfRef.AesCipherTop__DOT__sa00_sr) 
                                                 ^ 
                                                 (vlSelfRef.AesCipherTop__DOT__key_exp__DOT__w0 
                                                  >> 0x18U)));
    vlSelfRef.AesCipherTop__DOT__sa01_fark = (0xffU 
                                              & ((IData)(vlSelfRef.AesCipherTop__DOT__sa01_sr) 
                                                 ^ 
                                                 (vlSelfRef.AesCipherTop__DOT__key_exp__DOT__w1 
                                                  >> 0x18U)));
    vlSelfRef.AesCipherTop__DOT__sa02_fark = (0xffU 
                                              & ((IData)(vlSelfRef.AesCipherTop__DOT__sa02_sr) 
                                                 ^ 
                                                 (vlSelfRef.AesCipherTop__DOT__key_exp__DOT__w2 
                                                  >> 0x18U)));
    vlSelfRef.AesCipherTop__DOT__sa03_fark = (0xffU 
                                              & ((IData)(vlSelfRef.AesCipherTop__DOT__sa03_sr) 
                                                 ^ 
                                                 (vlSelfRef.AesCipherTop__DOT__key_exp__DOT__w3 
                                                  >> 0x18U)));
    vlSelfRef.AesCipherTop__DOT__sa10_fark = (0xffU 
                                              & ((IData)(vlSelfRef.AesCipherTop__DOT__sa10_sr) 
                                                 ^ 
                                                 (vlSelfRef.AesCipherTop__DOT__key_exp__DOT__w0 
                                                  >> 0x10U)));
    vlSelfRef.AesCipherTop__DOT__sa11_fark = (0xffU 
                                              & ((IData)(vlSelfRef.AesCipherTop__DOT__sa11_sr) 
                                                 ^ 
                                                 (vlSelfRef.AesCipherTop__DOT__key_exp__DOT__w1 
                                                  >> 0x10U)));
    vlSelfRef.AesCipherTop__DOT__sa12_fark = (0xffU 
                                              & ((IData)(vlSelfRef.AesCipherTop__DOT__sa12_sr) 
                                                 ^ 
                                                 (vlSelfRef.AesCipherTop__DOT__key_exp__DOT__w2 
                                                  >> 0x10U)));
    vlSelfRef.AesCipherTop__DOT__sa13_fark = (0xffU 
                                              & ((IData)(vlSelfRef.AesCipherTop__DOT__sa13_sr) 
                                                 ^ 
                                                 (vlSelfRef.AesCipherTop__DOT__key_exp__DOT__w3 
                                                  >> 0x10U)));
    vlSelfRef.AesCipherTop__DOT__sa20_fark = (0xffU 
                                              & ((IData)(vlSelfRef.AesCipherTop__DOT__sa20_sr) 
                                                 ^ 
                                                 (vlSelfRef.AesCipherTop__DOT__key_exp__DOT__w0 
                                                  >> 8U)));
    vlSelfRef.AesCipherTop__DOT__sa21_fark = (0xffU 
                                              & ((IData)(vlSelfRef.AesCipherTop__DOT__sa21_sr) 
                                                 ^ 
                                                 (vlSelfRef.AesCipherTop__DOT__key_exp__DOT__w1 
                                                  >> 8U)));
    vlSelfRef.AesCipherTop__DOT__sa22_fark = (0xffU 
                                              & ((IData)(vlSelfRef.AesCipherTop__DOT__sa22_sr) 
                                                 ^ 
                                                 (vlSelfRef.AesCipherTop__DOT__key_exp__DOT__w2 
                                                  >> 8U)));
    vlSelfRef.AesCipherTop__DOT__sa23_fark = (0xffU 
                                              & ((IData)(vlSelfRef.AesCipherTop__DOT__sa23_sr) 
                                                 ^ 
                                                 (vlSelfRef.AesCipherTop__DOT__key_exp__DOT__w3 
                                                  >> 8U)));
    vlSelfRef.AesCipherTop__DOT__sa30_fark = (0xffU 
                                              & ((IData)(AesCipherTop__DOT__sa30_sr) 
                                                 ^ vlSelfRef.AesCipherTop__DOT__key_exp__DOT__w0));
    vlSelfRef.AesCipherTop__DOT__sa00_mc = ((((([&]() {
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__16__a 
                                = vlSelfRef.AesCipherTop__DOT__sa00_sr;
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__16__shifted 
                                = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__16__a), 1U));
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__16__Vfuncout 
                                = (0xffU & ((0x80U 
                                             == (0x80U 
                                                 & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__16__a)))
                                             ? (0x1bU 
                                                ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__16__shifted))
                                             : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__16__shifted)));
                        }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__16__Vfuncout)) 
                                               ^ ([&]() {
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__17__a 
                                = vlSelfRef.AesCipherTop__DOT__sa10_sr;
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__17__shifted 
                                = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__17__a), 1U));
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__17__Vfuncout 
                                = (0xffU & ((0x80U 
                                             == (0x80U 
                                                 & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__17__a)))
                                             ? (0x1bU 
                                                ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__17__shifted))
                                             : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__17__shifted)));
                        }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__17__Vfuncout))) 
                                              ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa10_sr)) 
                                             ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa20_sr)) 
                                            ^ (IData)(AesCipherTop__DOT__sa30_sr));
    vlSelfRef.AesCipherTop__DOT__sa10_mc = (((((IData)(vlSelfRef.AesCipherTop__DOT__sa00_sr) 
                                               ^ ([&]() {
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__18__a 
                                = vlSelfRef.AesCipherTop__DOT__sa10_sr;
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__18__shifted 
                                = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__18__a), 1U));
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__18__Vfuncout 
                                = (0xffU & ((0x80U 
                                             == (0x80U 
                                                 & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__18__a)))
                                             ? (0x1bU 
                                                ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__18__shifted))
                                             : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__18__shifted)));
                        }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__18__Vfuncout))) 
                                              ^ ([&]() {
                        vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__19__a 
                            = vlSelfRef.AesCipherTop__DOT__sa20_sr;
                        vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__19__shifted 
                            = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__19__a), 1U));
                        vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__19__Vfuncout 
                            = (0xffU & ((0x80U == (0x80U 
                                                   & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__19__a)))
                                         ? (0x1bU ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__19__shifted))
                                         : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__19__shifted)));
                    }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__19__Vfuncout))) 
                                             ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa20_sr)) 
                                            ^ (IData)(AesCipherTop__DOT__sa30_sr));
    vlSelfRef.AesCipherTop__DOT__sa20_mc = (((((IData)(vlSelfRef.AesCipherTop__DOT__sa00_sr) 
                                               ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa10_sr)) 
                                              ^ ([&]() {
                        vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__20__a 
                            = vlSelfRef.AesCipherTop__DOT__sa20_sr;
                        vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__20__shifted 
                            = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__20__a), 1U));
                        vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__20__Vfuncout 
                            = (0xffU & ((0x80U == (0x80U 
                                                   & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__20__a)))
                                         ? (0x1bU ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__20__shifted))
                                         : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__20__shifted)));
                    }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__20__Vfuncout))) 
                                             ^ ([&]() {
                    vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__21__a 
                        = AesCipherTop__DOT__sa30_sr;
                    vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__21__shifted 
                        = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__21__a), 1U));
                    vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__21__Vfuncout 
                        = (0xffU & ((0x80U == (0x80U 
                                               & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__21__a)))
                                     ? (0x1bU ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__21__shifted))
                                     : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__21__shifted)));
                }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__21__Vfuncout))) 
                                            ^ (IData)(AesCipherTop__DOT__sa30_sr));
    vlSelfRef.AesCipherTop__DOT__sa30_mc = ((((([&]() {
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__22__a 
                                = vlSelfRef.AesCipherTop__DOT__sa00_sr;
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__22__shifted 
                                = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__22__a), 1U));
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__22__Vfuncout 
                                = (0xffU & ((0x80U 
                                             == (0x80U 
                                                 & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__22__a)))
                                             ? (0x1bU 
                                                ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__22__shifted))
                                             : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__22__shifted)));
                        }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__22__Vfuncout)) 
                                               ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa00_sr)) 
                                              ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa10_sr)) 
                                             ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa20_sr)) 
                                            ^ ([&]() {
                vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__23__a 
                    = AesCipherTop__DOT__sa30_sr;
                vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__23__shifted 
                    = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__23__a), 1U));
                vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__23__Vfuncout 
                    = (0xffU & ((0x80U == (0x80U & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__23__a)))
                                 ? (0x1bU ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__23__shifted))
                                 : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__23__shifted)));
            }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__23__Vfuncout)));
    vlSelfRef.AesCipherTop__DOT__sa31_fark = (0xffU 
                                              & ((IData)(vlSelfRef.AesCipherTop__DOT__sa31_sr) 
                                                 ^ vlSelfRef.AesCipherTop__DOT__key_exp__DOT__w1));
    vlSelfRef.AesCipherTop__DOT__sa01_mc = ((((([&]() {
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__24__a 
                                = vlSelfRef.AesCipherTop__DOT__sa01_sr;
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__24__shifted 
                                = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__24__a), 1U));
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__24__Vfuncout 
                                = (0xffU & ((0x80U 
                                             == (0x80U 
                                                 & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__24__a)))
                                             ? (0x1bU 
                                                ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__24__shifted))
                                             : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__24__shifted)));
                        }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__24__Vfuncout)) 
                                               ^ ([&]() {
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__25__a 
                                = vlSelfRef.AesCipherTop__DOT__sa11_sr;
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__25__shifted 
                                = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__25__a), 1U));
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__25__Vfuncout 
                                = (0xffU & ((0x80U 
                                             == (0x80U 
                                                 & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__25__a)))
                                             ? (0x1bU 
                                                ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__25__shifted))
                                             : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__25__shifted)));
                        }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__25__Vfuncout))) 
                                              ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa11_sr)) 
                                             ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa21_sr)) 
                                            ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa31_sr));
    vlSelfRef.AesCipherTop__DOT__sa11_mc = (((((IData)(vlSelfRef.AesCipherTop__DOT__sa01_sr) 
                                               ^ ([&]() {
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__26__a 
                                = vlSelfRef.AesCipherTop__DOT__sa11_sr;
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__26__shifted 
                                = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__26__a), 1U));
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__26__Vfuncout 
                                = (0xffU & ((0x80U 
                                             == (0x80U 
                                                 & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__26__a)))
                                             ? (0x1bU 
                                                ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__26__shifted))
                                             : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__26__shifted)));
                        }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__26__Vfuncout))) 
                                              ^ ([&]() {
                        vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__27__a 
                            = vlSelfRef.AesCipherTop__DOT__sa21_sr;
                        vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__27__shifted 
                            = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__27__a), 1U));
                        vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__27__Vfuncout 
                            = (0xffU & ((0x80U == (0x80U 
                                                   & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__27__a)))
                                         ? (0x1bU ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__27__shifted))
                                         : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__27__shifted)));
                    }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__27__Vfuncout))) 
                                             ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa21_sr)) 
                                            ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa31_sr));
    vlSelfRef.AesCipherTop__DOT__sa21_mc = (((((IData)(vlSelfRef.AesCipherTop__DOT__sa01_sr) 
                                               ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa11_sr)) 
                                              ^ ([&]() {
                        vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__28__a 
                            = vlSelfRef.AesCipherTop__DOT__sa21_sr;
                        vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__28__shifted 
                            = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__28__a), 1U));
                        vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__28__Vfuncout 
                            = (0xffU & ((0x80U == (0x80U 
                                                   & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__28__a)))
                                         ? (0x1bU ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__28__shifted))
                                         : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__28__shifted)));
                    }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__28__Vfuncout))) 
                                             ^ ([&]() {
                    vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__29__a 
                        = vlSelfRef.AesCipherTop__DOT__sa31_sr;
                    vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__29__shifted 
                        = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__29__a), 1U));
                    vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__29__Vfuncout 
                        = (0xffU & ((0x80U == (0x80U 
                                               & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__29__a)))
                                     ? (0x1bU ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__29__shifted))
                                     : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__29__shifted)));
                }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__29__Vfuncout))) 
                                            ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa31_sr));
    vlSelfRef.AesCipherTop__DOT__sa31_mc = ((((([&]() {
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__30__a 
                                = vlSelfRef.AesCipherTop__DOT__sa01_sr;
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__30__shifted 
                                = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__30__a), 1U));
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__30__Vfuncout 
                                = (0xffU & ((0x80U 
                                             == (0x80U 
                                                 & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__30__a)))
                                             ? (0x1bU 
                                                ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__30__shifted))
                                             : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__30__shifted)));
                        }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__30__Vfuncout)) 
                                               ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa01_sr)) 
                                              ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa11_sr)) 
                                             ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa21_sr)) 
                                            ^ ([&]() {
                vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__31__a 
                    = vlSelfRef.AesCipherTop__DOT__sa31_sr;
                vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__31__shifted 
                    = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__31__a), 1U));
                vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__31__Vfuncout 
                    = (0xffU & ((0x80U == (0x80U & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__31__a)))
                                 ? (0x1bU ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__31__shifted))
                                 : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__31__shifted)));
            }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__31__Vfuncout)));
    vlSelfRef.AesCipherTop__DOT__sa32_fark = (0xffU 
                                              & ((IData)(vlSelfRef.AesCipherTop__DOT__sa32_sr) 
                                                 ^ vlSelfRef.AesCipherTop__DOT__key_exp__DOT__w2));
    vlSelfRef.AesCipherTop__DOT__sa02_mc = ((((([&]() {
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__32__a 
                                = vlSelfRef.AesCipherTop__DOT__sa02_sr;
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__32__shifted 
                                = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__32__a), 1U));
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__32__Vfuncout 
                                = (0xffU & ((0x80U 
                                             == (0x80U 
                                                 & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__32__a)))
                                             ? (0x1bU 
                                                ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__32__shifted))
                                             : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__32__shifted)));
                        }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__32__Vfuncout)) 
                                               ^ ([&]() {
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__33__a 
                                = vlSelfRef.AesCipherTop__DOT__sa12_sr;
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__33__shifted 
                                = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__33__a), 1U));
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__33__Vfuncout 
                                = (0xffU & ((0x80U 
                                             == (0x80U 
                                                 & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__33__a)))
                                             ? (0x1bU 
                                                ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__33__shifted))
                                             : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__33__shifted)));
                        }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__33__Vfuncout))) 
                                              ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa12_sr)) 
                                             ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa22_sr)) 
                                            ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa32_sr));
    vlSelfRef.AesCipherTop__DOT__sa12_mc = (((((IData)(vlSelfRef.AesCipherTop__DOT__sa02_sr) 
                                               ^ ([&]() {
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__34__a 
                                = vlSelfRef.AesCipherTop__DOT__sa12_sr;
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__34__shifted 
                                = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__34__a), 1U));
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__34__Vfuncout 
                                = (0xffU & ((0x80U 
                                             == (0x80U 
                                                 & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__34__a)))
                                             ? (0x1bU 
                                                ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__34__shifted))
                                             : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__34__shifted)));
                        }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__34__Vfuncout))) 
                                              ^ ([&]() {
                        vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__35__a 
                            = vlSelfRef.AesCipherTop__DOT__sa22_sr;
                        vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__35__shifted 
                            = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__35__a), 1U));
                        vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__35__Vfuncout 
                            = (0xffU & ((0x80U == (0x80U 
                                                   & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__35__a)))
                                         ? (0x1bU ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__35__shifted))
                                         : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__35__shifted)));
                    }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__35__Vfuncout))) 
                                             ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa22_sr)) 
                                            ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa32_sr));
    vlSelfRef.AesCipherTop__DOT__sa22_mc = (((((IData)(vlSelfRef.AesCipherTop__DOT__sa02_sr) 
                                               ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa12_sr)) 
                                              ^ ([&]() {
                        vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__36__a 
                            = vlSelfRef.AesCipherTop__DOT__sa22_sr;
                        vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__36__shifted 
                            = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__36__a), 1U));
                        vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__36__Vfuncout 
                            = (0xffU & ((0x80U == (0x80U 
                                                   & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__36__a)))
                                         ? (0x1bU ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__36__shifted))
                                         : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__36__shifted)));
                    }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__36__Vfuncout))) 
                                             ^ ([&]() {
                    vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__37__a 
                        = vlSelfRef.AesCipherTop__DOT__sa32_sr;
                    vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__37__shifted 
                        = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__37__a), 1U));
                    vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__37__Vfuncout 
                        = (0xffU & ((0x80U == (0x80U 
                                               & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__37__a)))
                                     ? (0x1bU ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__37__shifted))
                                     : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__37__shifted)));
                }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__37__Vfuncout))) 
                                            ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa32_sr));
    vlSelfRef.AesCipherTop__DOT__sa32_mc = ((((([&]() {
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__38__a 
                                = vlSelfRef.AesCipherTop__DOT__sa02_sr;
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__38__shifted 
                                = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__38__a), 1U));
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__38__Vfuncout 
                                = (0xffU & ((0x80U 
                                             == (0x80U 
                                                 & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__38__a)))
                                             ? (0x1bU 
                                                ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__38__shifted))
                                             : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__38__shifted)));
                        }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__38__Vfuncout)) 
                                               ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa02_sr)) 
                                              ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa12_sr)) 
                                             ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa22_sr)) 
                                            ^ ([&]() {
                vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__39__a 
                    = vlSelfRef.AesCipherTop__DOT__sa32_sr;
                vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__39__shifted 
                    = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__39__a), 1U));
                vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__39__Vfuncout 
                    = (0xffU & ((0x80U == (0x80U & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__39__a)))
                                 ? (0x1bU ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__39__shifted))
                                 : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__39__shifted)));
            }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__39__Vfuncout)));
    vlSelfRef.AesCipherTop__DOT__sa33_fark = (0xffU 
                                              & ((IData)(AesCipherTop__DOT__sa33_sr) 
                                                 ^ vlSelfRef.AesCipherTop__DOT__key_exp__DOT__w3));
    vlSelfRef.AesCipherTop__DOT__sa03_mc = ((((([&]() {
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__40__a 
                                = vlSelfRef.AesCipherTop__DOT__sa03_sr;
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__40__shifted 
                                = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__40__a), 1U));
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__40__Vfuncout 
                                = (0xffU & ((0x80U 
                                             == (0x80U 
                                                 & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__40__a)))
                                             ? (0x1bU 
                                                ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__40__shifted))
                                             : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__40__shifted)));
                        }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__40__Vfuncout)) 
                                               ^ ([&]() {
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__41__a 
                                = vlSelfRef.AesCipherTop__DOT__sa13_sr;
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__41__shifted 
                                = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__41__a), 1U));
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__41__Vfuncout 
                                = (0xffU & ((0x80U 
                                             == (0x80U 
                                                 & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__41__a)))
                                             ? (0x1bU 
                                                ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__41__shifted))
                                             : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__41__shifted)));
                        }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__41__Vfuncout))) 
                                              ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa13_sr)) 
                                             ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa23_sr)) 
                                            ^ (IData)(AesCipherTop__DOT__sa33_sr));
    vlSelfRef.AesCipherTop__DOT__sa13_mc = (((((IData)(vlSelfRef.AesCipherTop__DOT__sa03_sr) 
                                               ^ ([&]() {
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__42__a 
                                = vlSelfRef.AesCipherTop__DOT__sa13_sr;
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__42__shifted 
                                = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__42__a), 1U));
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__42__Vfuncout 
                                = (0xffU & ((0x80U 
                                             == (0x80U 
                                                 & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__42__a)))
                                             ? (0x1bU 
                                                ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__42__shifted))
                                             : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__42__shifted)));
                        }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__42__Vfuncout))) 
                                              ^ ([&]() {
                        vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__43__a 
                            = vlSelfRef.AesCipherTop__DOT__sa23_sr;
                        vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__43__shifted 
                            = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__43__a), 1U));
                        vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__43__Vfuncout 
                            = (0xffU & ((0x80U == (0x80U 
                                                   & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__43__a)))
                                         ? (0x1bU ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__43__shifted))
                                         : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__43__shifted)));
                    }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__43__Vfuncout))) 
                                             ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa23_sr)) 
                                            ^ (IData)(AesCipherTop__DOT__sa33_sr));
    vlSelfRef.AesCipherTop__DOT__sa23_mc = (((((IData)(vlSelfRef.AesCipherTop__DOT__sa03_sr) 
                                               ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa13_sr)) 
                                              ^ ([&]() {
                        vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__44__a 
                            = vlSelfRef.AesCipherTop__DOT__sa23_sr;
                        vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__44__shifted 
                            = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__44__a), 1U));
                        vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__44__Vfuncout 
                            = (0xffU & ((0x80U == (0x80U 
                                                   & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__44__a)))
                                         ? (0x1bU ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__44__shifted))
                                         : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__44__shifted)));
                    }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__44__Vfuncout))) 
                                             ^ ([&]() {
                    vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__45__a 
                        = AesCipherTop__DOT__sa33_sr;
                    vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__45__shifted 
                        = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__45__a), 1U));
                    vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__45__Vfuncout 
                        = (0xffU & ((0x80U == (0x80U 
                                               & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__45__a)))
                                     ? (0x1bU ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__45__shifted))
                                     : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__45__shifted)));
                }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__45__Vfuncout))) 
                                            ^ (IData)(AesCipherTop__DOT__sa33_sr));
    vlSelfRef.AesCipherTop__DOT__sa33_mc = ((((([&]() {
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__46__a 
                                = vlSelfRef.AesCipherTop__DOT__sa03_sr;
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__46__shifted 
                                = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__46__a), 1U));
                            vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__46__Vfuncout 
                                = (0xffU & ((0x80U 
                                             == (0x80U 
                                                 & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__46__a)))
                                             ? (0x1bU 
                                                ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__46__shifted))
                                             : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__46__shifted)));
                        }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__46__Vfuncout)) 
                                               ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa03_sr)) 
                                              ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa13_sr)) 
                                             ^ (IData)(vlSelfRef.AesCipherTop__DOT__sa23_sr)) 
                                            ^ ([&]() {
                vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__47__a 
                    = AesCipherTop__DOT__sa33_sr;
                vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__47__shifted 
                    = (0xffU & VL_SHIFTL_III(8,8,32, (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__47__a), 1U));
                vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__47__Vfuncout 
                    = (0xffU & ((0x80U == (0x80U & (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__47__a)))
                                 ? (0x1bU ^ (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__47__shifted))
                                 : (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__47__shifted)));
            }(), (IData)(vlSelfRef.__Vfunc_AesCipherTop__DOT__Xtime__47__Vfuncout)));
    vlSelfRef.AesCipherTop__DOT__key_exp__DOT__nw1 
        = (vlSelfRef.AesCipherTop__DOT__key_exp__DOT__w1 
           ^ vlSelfRef.AesCipherTop__DOT__key_exp__DOT__nw0);
    vlSelfRef.AesCipherTop__DOT__key_exp__DOT__nw2 
        = (vlSelfRef.AesCipherTop__DOT__key_exp__DOT__w1 
           ^ (vlSelfRef.AesCipherTop__DOT__key_exp__DOT__w2 
              ^ vlSelfRef.AesCipherTop__DOT__key_exp__DOT__nw0));
    vlSelfRef.AesCipherTop__DOT__key_exp__DOT__nw3 
        = (vlSelfRef.AesCipherTop__DOT__key_exp__DOT__w3 
           ^ vlSelfRef.AesCipherTop__DOT__key_exp__DOT__nw2);
}

VL_ATTR_COLD void VAesCipherTop___024root___eval_triggers__stl(VAesCipherTop___024root* vlSelf);
VL_ATTR_COLD void VAesCipherTop___024root___eval_stl(VAesCipherTop___024root* vlSelf);

VL_ATTR_COLD bool VAesCipherTop___024root___eval_phase__stl(VAesCipherTop___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VAesCipherTop___024root___eval_phase__stl\n"); );
    VAesCipherTop__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Init
    CData/*0:0*/ __VstlExecute;
    // Body
    VAesCipherTop___024root___eval_triggers__stl(vlSelf);
    __VstlExecute = vlSelfRef.__VstlTriggered.any();
    if (__VstlExecute) {
        VAesCipherTop___024root___eval_stl(vlSelf);
    }
    return (__VstlExecute);
}

#ifdef VL_DEBUG
VL_ATTR_COLD void VAesCipherTop___024root___dump_triggers__act(VAesCipherTop___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VAesCipherTop___024root___dump_triggers__act\n"); );
    VAesCipherTop__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    if ((1U & (~ vlSelfRef.__VactTriggered.any()))) {
        VL_DBG_MSGF("         No triggers active\n");
    }
    if ((1ULL & vlSelfRef.__VactTriggered.word(0U))) {
        VL_DBG_MSGF("         'act' region trigger index 0 is active: @(posedge clk)\n");
    }
}
#endif  // VL_DEBUG

#ifdef VL_DEBUG
VL_ATTR_COLD void VAesCipherTop___024root___dump_triggers__nba(VAesCipherTop___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VAesCipherTop___024root___dump_triggers__nba\n"); );
    VAesCipherTop__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    if ((1U & (~ vlSelfRef.__VnbaTriggered.any()))) {
        VL_DBG_MSGF("         No triggers active\n");
    }
    if ((1ULL & vlSelfRef.__VnbaTriggered.word(0U))) {
        VL_DBG_MSGF("         'nba' region trigger index 0 is active: @(posedge clk)\n");
    }
}
#endif  // VL_DEBUG

VL_ATTR_COLD void VAesCipherTop___024root___ctor_var_reset(VAesCipherTop___024root* vlSelf) {
    VL_DEBUG_IF(VL_DBG_MSGF("+    VAesCipherTop___024root___ctor_var_reset\n"); );
    VAesCipherTop__Syms* const __restrict vlSymsp VL_ATTR_UNUSED = vlSelf->vlSymsp;
    auto& vlSelfRef = std::ref(*vlSelf).get();
    // Body
    vlSelf->clk = VL_RAND_RESET_I(1);
    vlSelf->rst = VL_RAND_RESET_I(1);
    vlSelf->ld = VL_RAND_RESET_I(1);
    vlSelf->done = VL_RAND_RESET_I(1);
    VL_RAND_RESET_W(128, vlSelf->key);
    VL_RAND_RESET_W(128, vlSelf->text_in);
    VL_RAND_RESET_W(128, vlSelf->text_out);
    vlSelf->AesCipherTop__DOT__dcnt = VL_RAND_RESET_I(4);
    vlSelf->AesCipherTop__DOT__ld_r = VL_RAND_RESET_I(1);
    vlSelf->AesCipherTop__DOT__done_r = VL_RAND_RESET_I(1);
    VL_RAND_RESET_W(128, vlSelf->AesCipherTop__DOT__text_in_r);
    vlSelf->AesCipherTop__DOT__sa00 = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa01 = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa02 = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa03 = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa10 = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa11 = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa12 = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa13 = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa20 = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa21 = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa22 = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa23 = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa30 = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa31 = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa32 = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa33 = VL_RAND_RESET_I(8);
    VL_RAND_RESET_W(128, vlSelf->AesCipherTop__DOT__text_out_r);
    vlSelf->AesCipherTop__DOT__sa00_sr = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa01_sr = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa02_sr = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa03_sr = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa10_sr = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa11_sr = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa12_sr = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa13_sr = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa20_sr = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa21_sr = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa22_sr = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa23_sr = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa31_sr = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa32_sr = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa00_mc = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa10_mc = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa20_mc = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa30_mc = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa01_mc = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa11_mc = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa21_mc = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa31_mc = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa02_mc = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa12_mc = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa22_mc = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa32_mc = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa03_mc = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa13_mc = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa23_mc = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa33_mc = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa00_fark = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa10_fark = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa20_fark = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa30_fark = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa01_fark = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa11_fark = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa21_fark = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa31_fark = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa02_fark = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa12_fark = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa22_fark = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa32_fark = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa03_fark = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa13_fark = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa23_fark = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__sa33_fark = VL_RAND_RESET_I(8);
    vlSelf->AesCipherTop__DOT__key_exp__DOT__w0 = VL_RAND_RESET_I(32);
    vlSelf->AesCipherTop__DOT__key_exp__DOT__w1 = VL_RAND_RESET_I(32);
    vlSelf->AesCipherTop__DOT__key_exp__DOT__w2 = VL_RAND_RESET_I(32);
    vlSelf->AesCipherTop__DOT__key_exp__DOT__w3 = VL_RAND_RESET_I(32);
    vlSelf->AesCipherTop__DOT__key_exp__DOT__rcnt = VL_RAND_RESET_I(4);
    vlSelf->AesCipherTop__DOT__key_exp__DOT__subword = VL_RAND_RESET_I(32);
    vlSelf->AesCipherTop__DOT__key_exp__DOT__nw0 = VL_RAND_RESET_I(32);
    vlSelf->AesCipherTop__DOT__key_exp__DOT__nw1 = VL_RAND_RESET_I(32);
    vlSelf->AesCipherTop__DOT__key_exp__DOT__nw2 = VL_RAND_RESET_I(32);
    vlSelf->AesCipherTop__DOT__key_exp__DOT__nw3 = VL_RAND_RESET_I(32);
    vlSelf->__Vfunc_AesCipherTop__DOT__AesSbox__11__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__AesSbox__15__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__16__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__16__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__16__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__17__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__17__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__17__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__18__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__18__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__18__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__19__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__19__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__19__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__20__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__20__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__20__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__21__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__21__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__21__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__22__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__22__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__22__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__23__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__23__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__23__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__24__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__24__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__24__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__25__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__25__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__25__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__26__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__26__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__26__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__27__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__27__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__27__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__28__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__28__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__28__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__29__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__29__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__29__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__30__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__30__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__30__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__31__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__31__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__31__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__32__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__32__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__32__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__33__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__33__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__33__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__34__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__34__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__34__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__35__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__35__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__35__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__36__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__36__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__36__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__37__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__37__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__37__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__38__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__38__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__38__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__39__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__39__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__39__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__40__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__40__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__40__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__41__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__41__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__41__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__42__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__42__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__42__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__43__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__43__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__43__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__44__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__44__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__44__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__45__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__45__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__45__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__46__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__46__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__46__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__47__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__47__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__Xtime__47__shifted = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__key_exp__DOT__AesSbox__48__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__key_exp__DOT__AesSbox__48__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__key_exp__DOT__AesSbox__49__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__key_exp__DOT__AesSbox__49__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__key_exp__DOT__AesSbox__50__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__key_exp__DOT__AesSbox__50__a = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__key_exp__DOT__AesSbox__51__Vfuncout = VL_RAND_RESET_I(8);
    vlSelf->__Vfunc_AesCipherTop__DOT__key_exp__DOT__AesSbox__51__a = VL_RAND_RESET_I(8);
    vlSelf->__Vtrigprevexpr___TOP__clk__0 = VL_RAND_RESET_I(1);
}
