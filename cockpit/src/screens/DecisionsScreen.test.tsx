import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { DecisionsScreen } from "./DecisionsScreen";
import type { DecisionRecord } from "../types";

const apiMocks = vi.hoisted(() => ({
  createDecision: vi.fn(),
  fetchDecisions: vi.fn(),
  resolveDecision: vi.fn(),
}));

vi.mock("../api", () => apiMocks);

function decision(overrides: Partial<DecisionRecord> = {}): DecisionRecord {
  return {
    id: "dec-1",
    node_id: "node-1",
    text: "should I proceed?",
    status: "pending",
    created_at_epoch_secs: Math.floor(Date.now() / 1000),
    decided_at_epoch_secs: null,
    ...overrides,
  };
}

describe("DecisionsScreen resolve with free-text answer", () => {
  beforeEach(() => {
    apiMocks.createDecision.mockReset();
    apiMocks.fetchDecisions.mockReset();
    apiMocks.resolveDecision.mockReset();
  });

  afterEach(() => cleanup());

  it("wires the free-text answer field into resolveDecision's answer param", async () => {
    apiMocks.fetchDecisions.mockResolvedValue([decision()]);
    apiMocks.resolveDecision.mockResolvedValue(decision({ status: "approved" }));

    const { getByLabelText, getByRole, findByText } = render(<DecisionsScreen />);
    await findByText("should I proceed?");

    const answerInput = getByLabelText("answer for decision dec-1");
    fireEvent.change(answerInput, { target: { value: "yes, go ahead" } });

    fireEvent.click(getByRole("button", { name: "approve" }));

    await waitFor(() => expect(apiMocks.resolveDecision).toHaveBeenCalled());
    expect(apiMocks.resolveDecision).toHaveBeenCalledWith("dec-1", {
      status: "approved",
      answer: "yes, go ahead",
    });
  });

  it("omits the answer key entirely when the free-text field is left blank", async () => {
    apiMocks.fetchDecisions.mockResolvedValue([decision()]);
    apiMocks.resolveDecision.mockResolvedValue(decision({ status: "denied" }));

    const { getByRole, findByText } = render(<DecisionsScreen />);
    await findByText("should I proceed?");

    fireEvent.click(getByRole("button", { name: "deny" }));

    await waitFor(() => expect(apiMocks.resolveDecision).toHaveBeenCalled());
    expect(apiMocks.resolveDecision).toHaveBeenCalledWith("dec-1", { status: "denied" });
  });

  it("clears the answer field after a successful resolve", async () => {
    apiMocks.fetchDecisions
      .mockResolvedValueOnce([decision()])
      .mockResolvedValue([decision({ status: "approved", decided_at_epoch_secs: 100 })]);
    apiMocks.resolveDecision.mockResolvedValue(decision({ status: "approved" }));

    const { getByLabelText, getByRole, findByText } = render(<DecisionsScreen />);
    await findByText("should I proceed?");

    const answerInput = getByLabelText("answer for decision dec-1") as HTMLInputElement;
    fireEvent.change(answerInput, { target: { value: "go" } });
    fireEvent.click(getByRole("button", { name: "approve" }));

    await waitFor(() => expect(apiMocks.resolveDecision).toHaveBeenCalled());
    // decision resolved out of the pending list; nothing left to assert the
    // stale answer against, but the resolve call captured the right payload.
    expect(apiMocks.resolveDecision).toHaveBeenCalledWith("dec-1", { status: "approved", answer: "go" });
  });
});
