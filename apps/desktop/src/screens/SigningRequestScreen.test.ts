import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import SigningRequestScreen from "./SigningRequestScreen.vue";

const review = {
  requestId: "123e4567-e89b-42d3-a456-426614174000",
  websiteOrigin: "https://example.com",
  documentName: "contract.pdf",
  documentSizeBytes: 1572864,
  status: "ready" as const,
  pages: [
    { pageNumber: 1, width: 612, height: 792 },
    { pageNumber: 2, width: 612, height: 792 },
  ],
};

describe("SigningRequestScreen", () => {
  beforeEach(() => {
    invoke.mockReset();
    invoke.mockImplementation((command: string) =>
      command === "render_pdf_review_page"
        ? Promise.resolve(new Uint8Array([1, 2, 3]))
        : Promise.resolve(),
    );
    URL.createObjectURL = vi.fn(() => "blob:preview");
    URL.revokeObjectURL = vi.fn();
    HTMLElement.prototype.scrollIntoView = vi.fn();
  });

  it("shows metadata, all page placeholders, and static-preview guidance", async () => {
    const wrapper = mount(SigningRequestScreen, { props: { review } });
    await flushPromises();

    expect(wrapper.text()).toContain("contract.pdf");
    expect(wrapper.text()).toContain("1.5 MB");
    expect(wrapper.text()).toContain("https://example.com");
    expect(wrapper.findAll(".pdf-page")).toHaveLength(2);
    expect(wrapper.findAll(".pdf-page img")).toHaveLength(2);
    expect(wrapper.text()).toContain(
      "Links, forms, scripts, and attachments are disabled",
    );
  });

  it("navigates the page stack and bounds zoom between 50 and 200 percent", async () => {
    const wrapper = mount(SigningRequestScreen, { props: { review } });
    await flushPromises();

    await wrapper.get('[aria-label="Next page"]').trigger("click");
    expect(HTMLElement.prototype.scrollIntoView).toHaveBeenCalledOnce();
    expect((wrapper.get("select").element as HTMLSelectElement).value).toBe(
      "2",
    );

    const zoomIn = wrapper.get('[aria-label="Zoom in"]');
    await zoomIn.trigger("click");
    await zoomIn.trigger("click");
    await zoomIn.trigger("click");
    await zoomIn.trigger("click");
    await flushPromises();
    expect(wrapper.get("output").text()).toBe("200%");
    expect(zoomIn.attributes("disabled")).toBeDefined();

    const scales = invoke.mock.calls
      .filter(([command]) => command === "render_pdf_review_page")
      .map(([, parameters]) => parameters.scale);
    expect(scales).toContain(2);
    expect(scales.every((scale) => scale >= 0.5 && scale <= 2)).toBe(true);
  });

  it("keeps Continue disabled until the backend reports ready", async () => {
    const wrapper = mount(SigningRequestScreen, {
      props: { review: { ...review, status: "preparing" as const } },
    });

    expect(
      wrapper.get(".action-buttons button:last-child").attributes("disabled"),
    ).toBeDefined();
    expect(wrapper.text()).toContain("Continue will unlock");

    await wrapper.setProps({ review: { ...review, status: "ready" as const } });
    await wrapper.get(".action-buttons button:last-child").trigger("click");
    expect(invoke).toHaveBeenCalledWith("continue_signing_request", {
      requestId: review.requestId,
    });
    await flushPromises();
    expect(wrapper.emitted("continued")).toHaveLength(1);
  });

  it("submits cancellation once and reports a command failure", async () => {
    let rejectCancellation: ((reason: Error) => void) | undefined;
    invoke.mockImplementation((command: string) => {
      if (command === "render_pdf_review_page")
        return Promise.resolve(new ArrayBuffer(1));
      return new Promise<void>((_resolve, reject) => {
        rejectCancellation = reject;
      });
    });
    const wrapper = mount(SigningRequestScreen, { props: { review } });
    const cancel = wrapper.get(".secondary-button");

    await cancel.trigger("click");
    await cancel.trigger("click");
    expect(
      invoke.mock.calls.filter(
        ([command]) => command === "cancel_signing_request",
      ),
    ).toHaveLength(1);
    expect(cancel.attributes("disabled")).toBeDefined();

    rejectCancellation?.(new Error("disconnected"));
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toContain(
      "could not be cancelled",
    );
  });
});
