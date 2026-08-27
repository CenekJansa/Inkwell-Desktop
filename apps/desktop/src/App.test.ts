import { mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ close: vi.fn() }),
}));

import App from "./App.vue";

describe("App", () => {
  it("shows direct-launch guidance", () => {
    const wrapper = mount(App);

    expect(wrapper.text()).toContain("Start signing from your browser");
    expect(wrapper.get("button").text()).toBe("Close");
  });
});
