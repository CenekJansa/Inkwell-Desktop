import { invoke } from "@tauri-apps/api/core";

import type { PdfReview } from "../types/pdfReview";

export function getPdfReview() {
  return invoke<PdfReview | null>("pdf_review_state");
}

export function renderPdfReviewPage(
  requestId: string,
  pageNumber: number,
  scale: number,
) {
  return invoke<Uint8Array | ArrayBuffer>("render_pdf_review_page", {
    requestId,
    pageNumber,
    scale,
  });
}

export function continueSigningRequest(requestId: string) {
  return invoke<void>("continue_signing_request", { requestId });
}

export function cancelSigningRequest(requestId: string) {
  return invoke<void>("cancel_signing_request", { requestId });
}
