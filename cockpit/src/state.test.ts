import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { selectCommandCenter } from "./state";

describe("selectCommandCenter", () => {
  it("selects a running command-center node before workers", () => {
    const selected = selectCommandCenter([
      { id: "worker", role_hint: "worker", liveness: "running" },
      { id: "cc", role_hint: "command-center", liveness: "running" },
    ]);

    expect(selected?.id).toBe("cc");
  });
});

// H5 — ntfy toast interval must not be torn down by channel state churn.
//
// Simulates the polling pattern: a channelsRef (plain object) is read inside
// a setInterval callback; the interval is set up once and its deps do NOT
// include channels. We verify the handler fires at the expected cadence even
// when channels would have been updated multiple times before the interval
// period elapses.
describe("H5 — ntfy toast interval stability under channel churn", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("fires the handler after 9s even when channels ref is updated every 6s", () => {
    // Simulate channelsRef — a mutable container updated externally (like
    // `channelsRef.current = channels` in the separate useEffect).
    const channelsRef = { current: [{ id: "ch-1", kind: "ntfy", live: true }] };

    const handler = vi.fn();

    // Set up the interval once (no channels dep, reads from ref).
    const t = setInterval(() => {
      // Reads current value from ref — never torn down between channel updates.
      const ntfyChannel = channelsRef.current.find((c) => c.kind === "ntfy" && c.live);
      if (ntfyChannel) handler(ntfyChannel.id);
    }, 9000);

    // Simulate two 6s polls updating the ref (new array reference each time,
    // as setChannels would produce).
    vi.advanceTimersByTime(6100);
    channelsRef.current = [{ id: "ch-1", kind: "ntfy", live: true }];

    // Handler must NOT have fired yet — 6.1s < 9s interval.
    expect(handler).not.toHaveBeenCalled();

    // Advance past the 9s mark.
    vi.advanceTimersByTime(3000); // total 9.1s

    expect(handler).toHaveBeenCalledOnce();
    expect(handler).toHaveBeenCalledWith("ch-1");

    clearInterval(t);
  });

  it("re-creating the interval (old bug behaviour) prevents the handler firing", () => {
    // This documents the BROKEN pattern where channels was in the dep array,
    // causing the interval to be torn down and reset every 6s.
    const handler = vi.fn();

    let t = setInterval(() => handler("fired"), 9000);

    // At 6s — simulate effect cleanup + re-setup (old buggy behaviour).
    vi.advanceTimersByTime(6100);
    clearInterval(t);
    t = setInterval(() => handler("fired"), 9000);

    // Advance to what would have been 9s from the original start.
    vi.advanceTimersByTime(2900); // total 9.0s but interval was reset at 6.1s

    // Handler has NOT fired — the reset stole the remaining time.
    expect(handler).not.toHaveBeenCalled();

    clearInterval(t);
  });
});
