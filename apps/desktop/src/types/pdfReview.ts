export type PdfReviewStatus = "preparing" | "ready" | "failed";

export interface PdfPageMetadata {
  pageNumber: number;
  width: number;
  height: number;
}

export interface PdfReview {
  requestId: string;
  websiteOrigin: string;
  documentName: string;
  documentSizeBytes: number;
  status: PdfReviewStatus;
  statusMessage?: string;
  pages: PdfPageMetadata[];
}
