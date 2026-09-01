import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import PdfReviewPage from "./PdfReviewPage.vue";

const props = {
  requestId: "request-1",
  page: { pageNumber: 1, width: 612, height: 792 },
  zoom: 100,
};

describe("PdfReviewPage", () => {
  beforeEach(() => {
    invoke.mockReset();
    URL.createObjectURL = vi.fn(() => "blob:rendered-page");
    URL.revokeObjectURL = vi.fn();
  });

  it("creates an image URL and revokes it when the page is removed", async () => {
    invoke.mockResolvedValue(new Uint8Array([137, 80, 78, 71]));
    const wrapper = mount(PdfReviewPage, { props });
    await flushPromises();

    expect(invoke).toHaveBeenCalledWith("render_pdf_review_page", {
      requestId: "request-1",
      pageNumber: 1,
      scale: 1,
    });
    expect(wrapper.get("img").attributes("src")).toBe("blob:rendered-page");

    wrapper.unmount();
    expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:rendered-page");
  });

  it("shows a page-local error and retries rendering", async () => {
    invoke.mockRejectedValueOnce(new Error("render failed"));
    invoke.mockResolvedValueOnce(new ArrayBuffer(4));
    const wrapper = mount(PdfReviewPage, { props });
    await flushPromises();

    expect(wrapper.get('[role="alert"]').text()).toContain(
      "Page 1 could not be displayed",
    );
    await wrapper.get("button").trigger("click");
    await flushPromises();

    expect(invoke).toHaveBeenCalledTimes(2);
    expect(wrapper.find("img").exists()).toBe(true);
  });

  it("revokes an obsolete render when zoom changes during loading", async () => {
    let resolveFirst: ((value: ArrayBuffer) => void) | undefined;
    invoke
      .mockReturnValueOnce(
        new Promise<ArrayBuffer>((resolve) => {
          resolveFirst = resolve;
        }),
      )
      .mockResolvedValueOnce(new ArrayBuffer(4));
    const wrapper = mount(PdfReviewPage, { props });

    await wrapper.setProps({ zoom: 125 });
    await flushPromises();
    resolveFirst?.(new ArrayBuffer(2));
    await flushPromises();

    expect(URL.createObjectURL).toHaveBeenCalledTimes(2);
    expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:rendered-page");
  });
});
