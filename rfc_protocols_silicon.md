## Summary

Modern SoC architectures targeting high-speed peripheral and serial subsystems (such as I3C HDR-DDR, CAN-FD, QSPI DDR, and USB 3.0 PIPE/UTMI) alongside advanced silicon implementations (multi-voltage I/O pads, dynamic well/body biasing, non-volatile MRAM/ReRAM memory, and power-domain isolation) require significant manual RTL boilerplate.

Without violating Arch's core principles — **standalone compiler (no JVM/Scala runtime), zero hidden hardware, dual-engine equivalence (sim == SV), and compile-time type safety** — this RFC proposes a modular, process-agnostic set of language primitives to eliminate repetitive boilerplate and provide front-end verification for complex protocols and physical silicon macros.

---

## 1. Stream / Decoupled Handshake Primitive (`stream<T>`)

### Problem

Wiring `valid`, `ready`, `data: T`, and backpressure/stall handshake logic across module and FIFO boundaries is the single largest source of repetitive boilerplate in streaming protocol controllers (I3C frame buffers, CAN message queues, QSPI burst engines, and USB PIPE packet streams). Hand-rolled ready/valid logic is also prone to combinatorial loop hazards and off-by-one skid buffer bugs.

### Proposal

Introduce a first-class unidirectional point-to-point handshake interface:

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

* **Semantics:** A `stream<T>` bundles `data: T` (forward), `valid: Bool` (forward), and `ready: Bool` (backward).
* **Handshake property:** Built-in `s.fire` is syntactic sugar for `(s.valid && s.ready)`.
* **Lowering:** Lowers cleanly in SystemVerilog to `logic [W-1:0] data`, `logic valid`, and `logic ready`. In native C++ simulation (`arch sim`), it lowers to direct signal structs without dynamic allocations or event simulator overhead. Seamlessly connects to `DualClockFifo`.

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

1. **`stream<T>` Implementation:** Should `stream<T>` be introduced as an intrinsic keyword primitive in the compiler grammar, or as a built-in standard library `bus` definition with a reserved `fire` helper?
2. **Tagged Union Bit Layout:** Should variant unions automatically pad to the widest variant width (`max_width(payloads)`), or should users have an optional explicit bit-width ceiling attribute?
3. **Physical Macro Library Packaging:** Should the physical cell abstractions (SIO pads, ICG clock gates, isolation cells) live as standard library modules under an `arch_stdcells` package, or use compiler-recognized intrinsic attributes?