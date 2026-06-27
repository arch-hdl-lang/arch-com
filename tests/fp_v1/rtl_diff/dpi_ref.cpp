// Host-float reference = arch-sim semantics (IEEE-754 single, RNE) + canonical NaN.
#include <cstdint>
#include <cstring>

static inline float u2f(uint32_t u){ float f; std::memcpy(&f,&u,4); return f; }
static inline uint32_t f2u(float f){ uint32_t u; std::memcpy(&u,&f,4); return u; }
static inline uint32_t canon(uint32_t u){
  if ((u & 0x7F800000u) == 0x7F800000u && (u & 0x007FFFFFu)) return 0x7FC00000u;
  return u;
}

extern "C" {
uint32_t dpi_mul(uint32_t a, uint32_t b){ return canon(f2u(u2f(a) * u2f(b))); }
uint32_t dpi_add(uint32_t a, uint32_t b){ return canon(f2u(u2f(a) + u2f(b))); }
uint32_t dpi_sub(uint32_t a, uint32_t b){ return canon(f2u(u2f(a) - u2f(b))); }
uint32_t dpi_fma(uint32_t a, uint32_t b, uint32_t c){ return canon(f2u(__builtin_fmaf(u2f(a), u2f(b), u2f(c)))); }
int      dpi_eq(uint32_t a, uint32_t b){ return u2f(a) == u2f(b); }
int      dpi_ne(uint32_t a, uint32_t b){ return u2f(a) != u2f(b); }
int      dpi_lt(uint32_t a, uint32_t b){ return u2f(a) <  u2f(b); }
int      dpi_le(uint32_t a, uint32_t b){ return u2f(a) <= u2f(b); }
int      dpi_gt(uint32_t a, uint32_t b){ return u2f(a) >  u2f(b); }
int      dpi_ge(uint32_t a, uint32_t b){ return u2f(a) >= u2f(b); }

// int -> f32 (RNE)
uint32_t dpi_s2f(int64_t v){ return canon(f2u((float)v)); }
uint32_t dpi_u2f(uint64_t v){ return canon(f2u((float)v)); }

// f32 -> int (toward zero, saturating to n bits, NaN -> type max) = arch-sim semantics
int64_t dpi_f2s(uint32_t x, int n){
  float f = u2f(x);
  __int128 smax = ((__int128)1 << (n-1)) - 1;
  __int128 smin = -((__int128)1 << (n-1));
  if (__builtin_isnan(f)) return (int64_t)smax;
  long double t = __builtin_truncl((long double)f);   // toward zero
  if (t > (long double)smax) return (int64_t)smax;    // also catches +inf
  if (t < (long double)smin) return (int64_t)smin;    // also catches -inf
  return (int64_t)(__int128)t;
}
// bf16 references = widen(canon) -> host float op -> narrow(bias-trick RNE)
static inline uint16_t narrow_bf16(uint32_t x){
  if ((x & 0x7F800000u) == 0x7F800000u && (x & 0x007FFFFFu)) return 0x7FC0;
  uint32_t r = x + 0x00007FFFu + ((x >> 16) & 1u);
  return (uint16_t)(r >> 16);
}
static inline uint32_t widen_bf16(uint16_t h){ return canon((uint32_t)h << 16); }
extern "C" {
uint32_t dpi_bf16_add(uint16_t a, uint16_t b){ return narrow_bf16(canon(f2u(u2f(widen_bf16(a)) + u2f(widen_bf16(b))))); }
uint32_t dpi_bf16_sub(uint16_t a, uint16_t b){ return narrow_bf16(canon(f2u(u2f(widen_bf16(a)) - u2f(widen_bf16(b))))); }
uint32_t dpi_bf16_mul(uint16_t a, uint16_t b){ return narrow_bf16(canon(f2u(u2f(widen_bf16(a)) * u2f(widen_bf16(b))))); }
uint32_t dpi_bf16_fma(uint16_t a, uint16_t b, uint16_t c){ return narrow_bf16(canon(f2u(__builtin_fmaf(u2f(widen_bf16(a)), u2f(widen_bf16(b)), u2f(widen_bf16(c)))))); }
}
extern "C" {
uint64_t dpi_f2u(uint32_t x, int n){
  float f = u2f(x);
  __int128 umax = ((__int128)1 << n) - 1;
  if (__builtin_isnan(f)) return (uint64_t)umax;
  if (f < 0) return 0;                                // negative (incl -inf) -> 0
  long double t = __builtin_truncl((long double)f);
  if (t > (long double)umax) return (uint64_t)umax;   // also catches +inf
  return (uint64_t)(__int128)t;
}
}
}
