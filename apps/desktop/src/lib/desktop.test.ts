import { describe, expect, it } from "vitest";
import { toCommandError } from "./desktop";

describe("command errors", () => {
  it("preserves structured fields from Tauri", () => {
    const error = toCommandError({
      code: "CANDIDATE_STALE",
      message: "candidate is stale",
      retryable: false,
    });

    expect(error.code).toBe("CANDIDATE_STALE");
    expect(error.message).toBe("candidate is stale");
    expect(error.retryable).toBe(false);
  });

  it("wraps string errors without string parsing", () => {
    const error = toCommandError("legacy string");
    expect(error.code).toBe("UNKNOWN");
    expect(error.message).toBe("legacy string");
  });
});
