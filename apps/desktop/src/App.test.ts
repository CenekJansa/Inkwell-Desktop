import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke, listen, listeners } = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  listeners: new Map<string, (event: unknown) => void>(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ close: vi.fn() }),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen }));

import App from "./App.vue";
import type { PdfReview } from "./types/pdfReview";

const review: PdfReview = {
  requestId: "123e4567-e89b-42d3-a456-426614174000",
  websiteOrigin: "https://example.com",
  documentName: "contract.pdf",
  documentSizeBytes: 245760,
  status: "preparing",
  pages: [
    { pageNumber: 1, width: 612, height: 792 },
    { pageNumber: 2, width: 792, height: 612 },
  ],
};

describe("App", () => {
  beforeEach(() => {
    invoke.mockReset();
    listen.mockReset();
    listeners.clear();
    listen.mockImplementation(
      (name: string, callback: (event: unknown) => void) => {
        listeners.set(name, callback);
        return Promise.resolve(vi.fn());
      },
    );
    URL.createObjectURL = vi.fn(() => "blob:preview");
    URL.revokeObjectURL = vi.fn();
  });

  it("shows loading before resolving to the direct-launch idle state", async () => {
    let resolveReview: ((value: null) => void) | undefined;
    invoke.mockReturnValue(
      new Promise<null>((resolve) => {
        resolveReview = resolve;
      }),
    );
    const wrapper = mount(App);

    expect(wrapper.text()).toContain("Preparing document review");
    resolveReview?.(null);
    await flushPromises();

    expect(wrapper.text()).toContain("Start signing from your browser");
  });

  it("shows a recoverable application error", async () => {
    invoke.mockRejectedValueOnce(new Error("offline"));
    const wrapper = mount(App);
    await flushPromises();

    expect(wrapper.get('[role="alert"]').text()).toContain("could not load");

    invoke.mockResolvedValueOnce(null);
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(wrapper.text()).toContain("Start signing from your browser");
  });

  it("renders metadata and the complete page stack for an active review", async () => {
    invoke.mockImplementation((command: string) =>
      command === "pdf_review_state"
        ? Promise.resolve(review)
        : Promise.resolve(new ArrayBuffer(4)),
    );
    const wrapper = mount(App);
    await flushPromises();

    expect(wrapper.text()).toContain("contract.pdf");
    expect(wrapper.text()).toContain("https://example.com");
    expect(wrapper.findAll(".pdf-page")).toHaveLength(2);
    expect(
      wrapper.get(".action-buttons button:last-child").attributes("disabled"),
    ).toBeDefined();
    expect(wrapper.find("embed").exists()).toBe(false);
    expect(wrapper.find("iframe").exists()).toBe(false);
  });

  it("refreshes readiness and invalidates only the matching request", async () => {
    let currentReview = review;
    invoke.mockImplementation((command: string) => {
      if (command === "pdf_review_state") return Promise.resolve(currentReview);
      return Promise.resolve(new ArrayBuffer(4));
    });
    const wrapper = mount(App);
    await flushPromises();

    currentReview = { ...review, status: "ready" };
    listeners.get("pdf-review-status-changed")?.({
      payload: { requestId: review.requestId },
    });
    await flushPromises();
    expect(
      wrapper.get(".action-buttons button:last-child").attributes("disabled"),
    ).toBeUndefined();

    listeners.get("signing-request-invalidated")?.({
      payload: { requestId: "another-request" },
    });
    await flushPromises();
    expect(wrapper.text()).toContain("contract.pdf");

    listeners.get("signing-request-invalidated")?.({
      payload: { requestId: review.requestId },
    });
    await flushPromises();
    expect(wrapper.text()).toContain("Start signing from your browser");
  });
});
