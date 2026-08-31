import { invoke } from "@tauri-apps/api/core";

import type { SigningRequestDisplay } from "../types/signingRequest";

export function getPendingSigningRequest() {
  return invoke<SigningRequestDisplay | null>("pending_signing_request");
}

export function cancelSigningRequest(requestId: string) {
  return invoke("cancel_signing_request", { requestId });
}
