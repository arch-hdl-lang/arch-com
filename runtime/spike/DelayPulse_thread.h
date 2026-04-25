// Hand-written spike: what the pre-lowering thread sim emitter should
// produce for tests/thread/wait_cycles.arch (module DelayPulse).
//
// Compare to the lowered-fsm sim output to validate runtime semantics
// before automating the codegen.

#pragma once

#include "../arch_thread_rt.h"
#include <cstdint>

class DelayPulse {
public:
    // Ports (mirror the .arch port list)
    uint8_t clk   = 0;
    uint8_t rst_n = 0;
    uint8_t start = 0;
    uint8_t pulse = 0;

    DelayPulse() {
        // Construct the thread coroutine and register it in the scheduler.
        _slot.thread = make_thread();
        _sched.slots.push_back(&_slot);
    }

    ~DelayPulse() { _slot.thread.destroy(); }

    // Combinational settle. For DelayPulse there's no combinational
    // logic in the module — only the thread drives `pulse` — so this
    // is a no-op. (The emitter will populate this for non-thread
    // comb blocks in other modules.)
    void eval() {}

    // Driven by the testbench whenever `clk` transitions. The thread
    // runs at posedge.
    void posedge_clk() {
        // Per-cycle default: zero all non-reg outputs the thread drives.
        // Matches the lowered-fsm semantic where state-local comb assigns
        // don't persist past the state. The coroutine re-asserts during
        // its span if it should still hold.
        // Emitter will derive this set from thread-body assignment
        // analysis (analogous to fsm `default` block).
        pulse = 0;
        // Async-active-low reset: re-create the thread.
        if (!rst_n) {
            // Reset side effects on outputs that the thread drives.
            // (In the lowered-fsm path this happens in the always_ff
            // reset arm. Here we mirror it explicitly.)
            pulse = 0;
            // Destroy + recreate the coroutine so it restarts from
            // the top. Cheap: the frame is one heap alloc per reset.
            _slot.thread.destroy();
            _slot.thread = make_thread();
            _slot.kind = arch_rt::WaitKind::Ready;
            _slot.cycles_remaining = 0;
            _slot.pred = nullptr;
            return;
        }
        _sched.tick();
    }

private:
    arch_rt::ThreadScheduler _sched;
    arch_rt::ThreadSlot      _slot;

    // The thread body — direct mechanical translation of:
    //   thread on clk rising, rst_n low
    //     wait until start;
    //     wait 5 cycle;
    //     pulse = 1;
    //     wait 1 cycle;
    //   end thread
    arch_rt::ArchThread make_thread() {
        co_await arch_rt::wait_until(&_slot, [this]{ return start != 0; });
        co_await arch_rt::wait_cycles(&_slot, 5);
        pulse = 1;
        co_await arch_rt::wait_cycles(&_slot, 1);
        co_return;
    }
};
