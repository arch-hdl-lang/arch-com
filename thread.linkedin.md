**A Better Way to Describe Multi-Cycle Hardware: ARCH Threads**

One of the most interesting features we have been building into ARCH HDL is the `thread` construct.

At first glance, it may look a little like HLS. You write something procedural:

```arch
thread dma on clk rising, rst high
  wait until start;
  addr <= base;
  count <= len;

  for i in 0..BURST_LEN-1
    bus.write(addr + i*4, data[i]);
    wait 1 cycle;
  end for
end thread dma
```

But this is not HLS in the traditional sense.

ARCH is not trying to infer arbitrary hardware from C-like software. It is still an HDL. The designer is describing clocked behavior, protocol waits, cycle boundaries, register updates, and concurrency explicitly.

The compiler’s job is narrower and, I think, more useful: lower a structured multi-cycle protocol into deterministic synthesizable RTL.

A `thread` in ARCH is a source-level control-flow construct for sequential hardware protocols. It supports:

- `wait until condition`
- `wait N cycle`
- `fork ... join`
- TLM-style method calls
- explicit register updates
- structured protocol sequencing

The compiler lowers this into a conventional FSM with state registers, counters, handshakes, muxing, and next-state logic.

So the designer gets the readability of a protocol script, while the output remains plain SystemVerilog that can be linted, simulated, synthesized, and formally checked.

The key distinction from HLS is intent.

HLS generally asks: “Can the compiler discover the hardware schedule?”

ARCH threads ask: “Can the compiler preserve the designer’s schedule while removing the boilerplate?”

That makes the lowering predictable. The source says where time passes. The compiler does not secretly invent a pipeline schedule, guess resource sharing, or reinterpret a software loop as hardware. It translates explicit temporal structure into RTL structure.

I have been calling this lowering approach:

**Temporal FSM Lowering**

or, more specifically:

**Structured Temporal Lowering**

The phrase captures the core idea: the source program is not a software algorithm to be scheduled; it is a structured temporal description of hardware behavior. The compiler lowers that temporal structure into an FSM.

A few other names I like:

- **Protocol-to-FSM Lowering**
- **Temporal Control Lowering**
- **Clocked Thread Lowering**
- **Schedule-Preserving Thread Lowering**
- **Explicit-Time Lowering**

My favorite is probably **Schedule-Preserving Thread Lowering** when explaining the distinction from HLS, and **Structured Temporal Lowering** as the compiler pass name.

The practical benefit is significant.

Many hardware blocks contain control protocols that are painful to maintain as hand-written FSMs: DMA engines, bus adapters, initialization sequences, retry loops, command queues, microcoded control, and transaction-level interfaces.

In plain RTL, the intent often disappears into state encodings and next-state assignments.

With ARCH threads, the source keeps the protocol shape visible:

```arch
thread requester on clk rising, rst high
  req_valid <= 1;
  wait until req_ready;
  req_valid <= 0;

  wait 2 cycle;

  rsp_ready <= 1;
  wait until rsp_valid;
  data <= rsp_data;
  rsp_ready <= 0;
end thread requester
```

The compiler emits the FSM. The human reads the protocol.

That is the design point of ARCH: not “write software and hope hardware comes out,” but “write hardware intent at the right level and let the compiler handle the mechanical lowering.”

For LLM-generated HDL, this matters even more.

LLMs are much better at writing structured protocol descriptions than they are at consistently producing perfect hand-coded FSM bookkeeping. A construct like `thread` gives the model a safer abstraction: explicit time, explicit waits, explicit concurrency, and compiler-checked lowering.

The result is still hardware. Just with fewer places for accidental complexity to hide.