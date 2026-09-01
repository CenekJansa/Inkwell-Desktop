import { mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ close: vi.fn() }),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(vi.fn()),
}));

import App from "./App.vue";

describe("App", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("shows direct-launch guidance", async () => {
    invoke.mockResolvedValue(null);
    const wrapper = mount(App);

    await vi.waitFor(() =>
      expect(wrapper.text()).toContain("Start signing from your browser"),
    );
    expect(wrapper.get("button").text()).toBe("Close");
  });

  it("shows pending request metadata from the desktop harness", async () => {
    invoke.mockResolvedValue({
      requestId: "123e4567-e89b-42d3-a456-426614174000",
      websiteOrigin: "https://example.com",
      documentName: "contract.pdf",
    });
    const wrapper = mount(App);

    await vi.waitFor(() => expect(wrapper.text()).toContain("contract.pdf"));
    expect(wrapper.text()).toContain("https://example.com");
  });
});
