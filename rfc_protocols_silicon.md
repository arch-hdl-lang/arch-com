## Summary

Modern SoC architectures targeting high-speed peripheral and serial subsystems (such as I3C HDR-DDR, CAN-FD, QSPI DDR, and USB 3.0 PIPE/UTMI) alongside advanced silicon implementations (multi-voltage I/O pads, dynamic well/body biasing, non-volatile MRAM/ReRAM memory, and power-domain isolation) require significant manual RTL boilerplate.

Without violating Arch's core principles — **standalone compiler (no JVM/Scala runtime), zero hidden hardware, dual-engine equivalence (sim == SV), and compile-time type safety** — this RFC covers two protocol-level gaps (§2 tagged unions with payloads, §3 physical silicon and power abstractions) and one narrower clarification request about where the existing handshake constructs apply (§1).

> **Scope note.** §1 originally proposed a new `stream<T>` primitive. Review showed `handshake_channel`, `credit_channel`, `Token`, and `fifo` already cover the semantics, so §1 is revised to a clarification request rather than a new-primitive proposal. §2 and §3 are unaffected.

---

## 1. Standalone Point-to-Point Stream Ports (clarification request, not a new primitive)

### Status after review

An earlier draft of this section proposed `stream<T>` as a *new* first-class primitive. That was an overclaim, and this revision withdraws it. ARCH already ships the core of what a streaming port needs:

* **`handshake_channel`** (§18b, nested in `bus`) — compiler-emitted valid/ready protocol wires plus Tier-2 protocol SVA.
* **`credit_channel`** (§18c, nested in `bus`) — compiler-owned sender credit counter, receiver FIFO, and credit-return wires, with Lean/SMT construct proofs.
* **`Token`** (§3.1) — a zero-width pure handshake carrier holding only `valid`/`ready`.
* **`fifo`** (§8.2) — dual-clock FIFO with a verified push/pop handshake contract.

The residual gap is narrower than the original draft implied, and it is a question about *placement*, not about missing semantics.

### The actual question

Both `handshake_channel` and `credit_channel` are **bus sub-constructs only** — §18c states plainly that "there is no standalone form, credit channels always nest inside a `bus`." A module that needs a single point-to-point streaming port with no address decode, no second channel, and no bus-level arbitration must therefore declare a one-channel `bus` purely to acquire handshake semantics.

That is the same boilerplate the construct was introduced to remove, and it interacts awkwardly with two things the language already supports well:

1. **Type parameters.** `bus` sub-constructs are declared per-channel with concrete port lists; a generic `SkidBuffer<T: type>` cannot express "a channel whose payload is `T`" without either monomorphising per instantiation or falling back to a raw `Vec<UInt<8>,N>` port and hand-written valid/ready.
2. **`Vec<T,N>` payloads.** A burst link whose payload is an array element has no way to state that the handshake covers the whole element rather than its bits.

### What would resolve it

Either of these would close the question without new semantics:

* **(a)** Confirm that a single-channel `bus` *is* the intended idiom for point-to-point streaming, and add a short spec example plus a note in §18b/§18c saying so. This costs no compiler work.
* **(b)** Confirm that a standalone port form (a `stream<T>`-alike that lowers to exactly the same three signals `handshake_channel` already emits, minus the bus wrapper) is in scope for `doc/archive/plan_bus_unification.md`. This is a parser/elab change, not new hardware semantics.

### Illustrative target (only if (b) is in scope)

The shape below is *aspirational* and shown only to make the gap concrete — it is not a proposed grammar:

```arch
module SkidBuffer<T: type>
  port clk: in Clock<SysDomain>;
  port rst: in Reset<Sync, High>;
  port s_in: in stream<T>;
  port m_out: out stream<T>;

  reg buffer: T;
  reg has_buffer: Bool reset rst => false;

  comb
    s_in.ready = !has_buffer || m_out.ready;
    m_out.valid = s_in.valid || has_buffer;
    m_out.data = has_buffer ? buffer : s_in.data;
  end comb

  seq on clk rising
    if s_in.fire && !m_out.ready
      buffer <= s_in.data;
      has_buffer <= true;
    else if m_out.ready
      has_buffer <= false;
    end if
  end seq
end module SkidBuffer
```

If this were adopted, `stream<T>` would be *syntactic sugar over the existing `handshake_channel` lowering* — same `data`/`valid`/`ready` signals, same `fire` predicate, same SVA — with no new semantics and no change to `sim == SV` equivalence. That constraint is what keeps it inside Arch's core principles.

---

## 2. Tagged Unions / Algebraic Data Types (Enums with Payloads)

### Problem

When decoding multi-format packet headers, frames, and command codes (e.g., I3C Common Command Codes [CCC], CAN arbitration ID/DLC/Data payloads, USB transaction tokens, and QSPI command phases), designers currently write manual bit-slicing (`frame[31:24]`), brittle bitmasks, and error-prone multiplexers.

### Proposal

Extend Arch's existing `enum` grammar to allow structured variant payloads:

```arch
enum Packet {
  Idle,
  Header(UInt<16>),
  Payload { id: UInt<11>, dlc: UInt<4>, data: Vec<UInt<8>, 8> },
  Status(UInt<8>),
  Error
}

comb
  match rx_pkt
    Packet::Idle => ...
    Packet::Header(hdr) => ...
    Packet::Payload{id, dlc, data} => ...
    Packet::Status(code) => ...
    Packet::Error => ...
  end match
end comb
```

* **Exhaustiveness:** Enforced by Arch's existing pattern match exhaustive analyzer.
* **Lowering:** In SystemVerilog, lowers directly to a synthesizable `typedef union packed` accompanied by a discriminant enum tag. In native C++ simulation, lowers to a `struct { uint8_t tag; union { ... } }`.

---

## 3. Process-Agnostic Physical Silicon & Power Abstractions

To allow hardware IP designed in Arch to deploy across modern silicon processes (bulk CMOS, FD-SOI, BiCMOS, etc.) without proprietary foundry secret sauce or hardcoded node recipes, we propose standardizing four process-agnostic hardware building blocks:

### A. Special I/O (SIO) Triad / Multi-Mode Pad Primitive

Modern serial buses require dynamic pad control (e.g., I3C switching dynamically between open-drain at 12.5 MHz and push-pull up to 33 MHz; CAN bus driving differential high-voltage transceivers; QSPI running high-slew DDR).

* **Interface Specification:**
  * Direction: Bidirectional `inout` with dynamic output enable (`oe`).
  * Mode: Open-drain vs. Push-pull selection.
  * Programmable drive strength (e.g., discrete steps: 2mA, 4mA, 8mA, 12mA).
  * Slew rate control and level-shifting domain isolation (e.g., 3.3V pad domain vs 1.2V core domain).
* **Emission:** Emits clean portable SystemVerilog triad port models (`in`, `out`, `oe`, `drive_sel`) with standard blackbox wrappers for target ASIC pad cells or FPGA I/O buffers.

### B. Closed-Loop Adaptive Body Biasing (ABB) / Well-Bias Control

For high-efficiency / low-power silicon architectures (including FD-SOI and advanced bulk):

* Standardized digital control loop pattern that interfaces with process/leakage/ring-oscillator monitors and adjusts forward/reverse body-bias (FBB/RBB) DAC codes to compensate for PVT (Process, Voltage, Temperature) corners.
* Purely digital, synthesizable controller module requiring zero analog behavioral modeling inside the RTL.

### C. Non-Volatile Memory (MRAM / ReRAM) Timing & ECC Primitives

Emerging memory controllers require asymmetric read/write duration and robust error mitigation:

* Built-in parameterized write-pulse FSM ensuring independent write duration, soak times, and pre-charge recovery cycles.
* Integrated SEC-DED (Single Error Correction, Double Error Detection) Hamming codec primitives (e.g., 32-bit data + 7 parity bits = 39-bit codeword) verifiable at compile time.

### D. Power Isolation and Integrated Clock Gating (ICG)

* Explicit compiler primitives for power domain isolation (`isocel` clamp-high / clamp-low) to prevent floating inputs when a power domain is switched off.
* Scan-bypass integrated clock gating (ICG) primitive ensuring glitch-free clock gating with automatic test-mode scan enable (`test_en`) override.

---

## Soundness & Guardrails

* **Zero Host-Language Runtime:** Implemented entirely within the standalone Rust compiler; no Scala/JVM or external script execution during build or check.
* **Dual-Engine Consistency:** Both backends (`arch build` SystemVerilog and `arch sim` C++ testbench) model identical signal transactions, preventing sim-vs-synthesis divergence.
* **Process Independence:** No vendor PDK names, proprietary technology rules, or fab-specific geometry data are hardcoded in the compiler. All physical constructs rely on clean structural abstractions.

---

## Open Decisions for Maintainer Sign-off

1. **Point-to-Point Streaming (clarification):** Is a single-channel `bus` the intended idiom for a standalone streaming port, or should a `stream<T>`-alike be added that lowers to exactly the signals `handshake_channel` already emits? If the former, a spec example in §18b/§18c would settle it with no compiler change.
2. **Tagged Union Bit Layout:** Should variant unions automatically pad to the widest variant width (`max_width(payloads)`), or should users have an optional explicit bit-width ceiling attribute?
3. **Physical Macro Library Packaging:** Should the physical cell abstractions (SIO pads, ICG clock gates, isolation cells) live as standard library modules under an `arch_stdcells` package, or use compiler-recognized intrinsic attributes?