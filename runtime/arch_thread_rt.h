// arch_thread_rt.h — Minimal coroutine runtime for pre-lowering thread sim.
//
// Design (Phase 1 spike, single-thread, single-clock-domain):
//   - Each `thread` block compiles to a C++ coroutine returning `ArchThread`.
//   - The scheduler exposes one method per clock edge: `tick()`. It resumes
//     every coroutine that is ready (predicate satisfied or cycle deadline
//     reached); coroutines run statements until they hit `co_await wait_*`.
//   - Statements between `wait`s execute atomically at one posedge —
//     matching the lowered-fsm semantics where one state's seq block runs
//     in one always_ff iteration.
//   - Awaiters:
//       * `wait_until(pred)`  — suspend; resume on next posedge where
//                                pred() returns true.
//       * `wait_cycles(N)`    — suspend; resume after N posedges (N>=1).
//
// Not yet handled in this spike: fork/join, multiple threads,
// resource locks, cross-thread signal access, parallel execution.
// Those land in later phases.

#pragma once

#include <coroutine>
#include <functional>
#include <cstdint>
#include <vector>

namespace arch_rt {

// Forward decls
struct ThreadScheduler;

// Promise type — minimal. The coroutine returns void; the handle is
// owned by the scheduler.
struct ArchThread {
    struct promise_type {
        ArchThread get_return_object() {
            return ArchThread{ std::coroutine_handle<promise_type>::from_promise(*this) };
        }
        std::suspend_always initial_suspend() noexcept { return {}; }
        std::suspend_always final_suspend()   noexcept { return {}; }
        void return_void() noexcept {}
        void unhandled_exception() { std::terminate(); }
    };
    std::coroutine_handle<promise_type> h;
    bool done() const { return !h || h.done(); }
    void resume() { if (h && !h.done()) h.resume(); }
    void destroy() { if (h) { h.destroy(); h = nullptr; } }
};

// State a suspended thread is parked on.
//   Ready       — not currently suspended; will run on next tick().
//   WaitUntil   — resume when pred() returns true at a posedge.
//   WaitCycles  — resume after `cycles_remaining` posedges.
//   Done        — coroutine finished.
enum class WaitKind : uint8_t { Ready, WaitUntil, WaitCycles, Done };

// One scheduled thread. The scheduler owns these; awaiters mutate the
// fields when a coroutine suspends.
struct ThreadSlot {
    ArchThread thread;
    WaitKind   kind = WaitKind::Ready;
    uint32_t   cycles_remaining = 0;
    std::function<bool()> pred;  // for WaitUntil
};

// Awaiter: `co_await wait_until(pred)`. Captures the predicate by value;
// the awaiter object lives in the coroutine frame for the duration of
// the suspend, so the lambda's captures must outlive that — which is
// trivially true since the caller is the same coroutine.
struct WaitUntilAwaiter {
    std::function<bool()> pred;
    ThreadSlot* slot;
    bool await_ready() noexcept {
        // Don't short-circuit: even if pred is true now, we want the
        // semantics of "resumes at the *next* posedge where pred is true",
        // matching the lowered-fsm behavior.
        return false;
    }
    void await_suspend(std::coroutine_handle<>) noexcept {
        slot->kind = WaitKind::WaitUntil;
        slot->pred = std::move(pred);
    }
    void await_resume() noexcept {}
};

// Awaiter: `co_await wait_cycles(N)`. N must be >= 1.
struct WaitCyclesAwaiter {
    uint32_t n;
    ThreadSlot* slot;
    bool await_ready() noexcept { return n == 0; }
    void await_suspend(std::coroutine_handle<>) noexcept {
        slot->kind = WaitKind::WaitCycles;
        slot->cycles_remaining = n;
    }
    void await_resume() noexcept {}
};

// Scheduler: one per module instance. Owns all threads in that module.
struct ThreadScheduler {
    std::vector<ThreadSlot*> slots;

    // Called by the module's posedge handler after combinational settle.
    // For each slot:
    //   - Done:        skip.
    //   - WaitCycles:  decrement; if hits 0, mark Ready.
    //   - WaitUntil:   evaluate pred; if true, mark Ready.
    //   - Ready:       (only first tick, or just-marked-ready) resume the
    //                  coroutine until it suspends or finishes.
    void tick() {
        // Semantic: `wait until cond` blocks for AT LEAST one posedge
        // FROM THE MOMENT OF SUSPENSION. Same for `wait_cycles(N)`.
        // Concretely:
        //   - A slot freshly suspended this tick (resumed[i]=true)
        //     will NOT have its new pred re-evaluated this tick →
        //     min 1 cycle wait from suspension.
        //   - A slot that was already WaitUntil at tick start
        //     (resumed[i]=false) CAN fire mid-tick when its pred
        //     becomes true due to another slot's resume — it has
        //     already been waiting ≥1 cycle, so satisfying it now
        //     respects the min-1 rule.
        // This is essential for fork-join: the parent's "all branches
        // Done" pred must fire the same tick branches finish, matching
        // the lowered-fsm rule that an unconditional state transition
        // fires at the posedge after the predecessor state's residency.
        std::vector<bool> resumed(slots.size(), false);

        // Pass 1: advance wait conditions based on prior-tick state.
        for (auto* s : slots) {
            if (s->kind == WaitKind::WaitCycles) {
                if (s->cycles_remaining > 0) --s->cycles_remaining;
                if (s->cycles_remaining == 0) s->kind = WaitKind::Ready;
            } else if (s->kind == WaitKind::WaitUntil) {
                if (s->pred && s->pred()) s->kind = WaitKind::Ready;
            }
        }
        // Pass 2 (iterated): resume Ready slots, then re-check preds
        // for slots that did NOT resume this tick. The resumed[] guard
        // prevents freshly suspended slots from re-firing same tick.
        bool changed = true;
        while (changed) {
            changed = false;
            for (size_t i = 0; i < slots.size(); ++i) {
                if (slots[i]->kind == WaitKind::Ready && !resumed[i]) {
                    resumed[i] = true;
                    slots[i]->thread.resume();
                    if (slots[i]->thread.done()) slots[i]->kind = WaitKind::Done;
                    changed = true;
                }
            }
            for (size_t i = 0; i < slots.size(); ++i) {
                if (!resumed[i] && slots[i]->kind == WaitKind::WaitUntil) {
                    if (slots[i]->pred && slots[i]->pred()) {
                        slots[i]->kind = WaitKind::Ready;
                        changed = true;
                    }
                }
            }
        }
    }

    bool all_done() const {
        for (auto* s : slots) if (s->kind != WaitKind::Done) return false;
        return true;
    }
};

// Convenience constructors. Pass the slot the coroutine is parked in so
// the awaiter knows where to write its suspend state.
inline WaitUntilAwaiter  wait_until (ThreadSlot* s, std::function<bool()> p) { return {std::move(p), s}; }
inline WaitCyclesAwaiter wait_cycles(ThreadSlot* s, uint32_t n)              { return {n, s}; }

} // namespace arch_rt
