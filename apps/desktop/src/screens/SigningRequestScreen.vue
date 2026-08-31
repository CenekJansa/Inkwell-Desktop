<script setup lang="ts">
import { ref } from "vue";

import { cancelSigningRequest } from "../api/walkingSkeleton";
import type { SigningRequestDisplay } from "../types/signingRequest";

const props = defineProps<{ request: SigningRequestDisplay }>();
const emit = defineEmits<{ cancelled: [] }>();
const cancelling = ref(false);
const cancellationFailed = ref(false);

async function cancelRequest() {
  if (cancelling.value) return;

  cancelling.value = true;
  cancellationFailed.value = false;
  try {
    await cancelSigningRequest(props.request.requestId);
    emit("cancelled");
  } catch {
    cancellationFailed.value = true;
    cancelling.value = false;
  }
}
</script>

<template>
  <main class="request-screen">
    <header class="request-header">
      <p class="product-name">Inkwell Desktop</p>
      <p class="request-step">Document review</p>
    </header>

    <section class="request-card" aria-labelledby="request-title">
      <div class="request-marker" aria-hidden="true">01</div>
      <div>
        <p class="eyebrow">Awaiting your review</p>
        <h1 id="request-title">{{ request.documentName }}</h1>
        <p class="origin-label">Requesting website</p>
        <p class="origin-value">{{ request.websiteOrigin }}</p>
        <p class="origin-note">
          This website address is supplied by the approved browser extension.
        </p>
      </div>
    </section>

    <footer class="request-actions">
      <p>
        No document preview or signing action is available in this milestone.
      </p>
      <button type="button" :disabled="cancelling" @click="cancelRequest">
        {{ cancelling ? "Cancelling…" : "Cancel request" }}
      </button>
      <p v-if="cancellationFailed" class="action-error" role="alert">
        The request could not be cancelled. Return to Chrome and try again.
      </p>
    </footer>
  </main>
</template>
