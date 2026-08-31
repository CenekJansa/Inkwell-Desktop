import { mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import SigningRequestScreen from "./SigningRequestScreen.vue";

const request = {
  requestId: "123e4567-e89b-42d3-a456-426614174000",
  websiteOrigin: "https://example.com",
  documentName: "contract.pdf",
};

describe("SigningRequestScreen", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("displays the extension-supplied origin and document name", () => {
    const wrapper = mount(SigningRequestScreen, { props: { request } });

    expect(wrapper.text()).toContain("contract.pdf");
    expect(wrapper.text()).toContain("https://example.com");
    expect(wrapper.text()).toContain(
      "supplied by the approved browser extension",
    );
  });

  it("returns one cancellation and blocks duplicate submissions", async () => {
    let resolveCancellation: (() => void) | undefined;
    invoke.mockReturnValue(
      new Promise<void>((resolve) => {
        resolveCancellation = resolve;
      }),
    );
    const wrapper = mount(SigningRequestScreen, { props: { request } });
    const button = wrapper.get("button");

    await button.trigger("click");
    await button.trigger("click");

    expect(invoke).toHaveBeenCalledOnce();
    expect(invoke).toHaveBeenCalledWith("cancel_signing_request", {
      requestId: request.requestId,
    });
    expect(button.attributes("disabled")).toBeDefined();

    resolveCancellation?.();
    await vi.waitFor(() =>
      expect(wrapper.emitted("cancelled")).toHaveLength(1),
    );
  });
});
