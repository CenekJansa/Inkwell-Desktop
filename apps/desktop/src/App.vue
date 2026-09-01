<script setup lang="ts">
import { listen, type Event, type UnlistenFn } from "@tauri-apps/api/event";
import { onMounted, onUnmounted, ref } from "vue";

import { getPdfReview } from "./api/pdfReview";
import SigningRequestScreen from "./screens/SigningRequestScreen.vue";
import WaitingScreen from "./screens/WaitingScreen.vue";
import type { PdfReview } from "./types/pdfReview";

type AppPhase = "loading" | "idle" | "error" | "active";
type RequestEvent = Event<{ requestId?: string }>;

const phase = ref<AppPhase>("loading");
const review = ref<PdfReview | null>(null);
const loadError = ref("");
const unlisteners: UnlistenFn[] = [];
let loadSequence = 0;
let mounted = true;

async function loadReview(showLoading = true) {
  const sequence = ++loadSequence;
  if (showLoading) phase.value = "loading";
  loadError.value = "";

  try {
    const nextReview = await getPdfReview();
    if (!mounted || sequence !== loadSequence) return;
    review.value = nextReview;
    phase.value = nextReview ? "active" : "idle";
  } catch {
    if (!mounted || sequence !== loadSequence) return;
    review.value = null;
    loadError.value = "Inkwell could not load the document review.";
    phase.value = "error";
  }
}

function requestMatches(event: RequestEvent) {
  return (
    !event.payload?.requestId ||
    event.payload.requestId === review.value?.requestId
  );
}

function invalidateReview(event: RequestEvent) {
  if (!requestMatches(event)) return;
  loadSequence += 1;
  review.value = null;
  phase.value = "idle";
}

async function registerListeners() {
  const listeners = await Promise.all([
    listen("signing-request-available", () => loadReview()),
    listen<RequestEvent["payload"]>(
      "signing-request-invalidated",
      invalidateReview,
    ),
    listen<RequestEvent["payload"]>("pdf-review-status-changed", (event) => {
      if (requestMatches(event)) void loadReview(false);
    }),
  ]);

  if (!mounted) {
    listeners.forEach((unlisten) => unlisten());
    return;
  }
  unlisteners.push(...listeners);
  await loadReview();
}

onMounted(() => {
  void registerListeners().catch(() => {
    if (!mounted) return;
    loadError.value =
      "Inkwell could not connect to the document review service.";
    phase.value = "error";
  });
});

onUnmounted(() => {
  mounted = false;
  loadSequence += 1;
  unlisteners.forEach((unlisten) => unlisten());
});
</script>

<template>
  <main v-if="phase === 'loading'" class="app-state-screen" aria-busy="true">
    <section class="app-state-card" role="status">
      <p class="product-name">Inkwell Desktop</p>
      <span class="state-spinner" aria-hidden="true"></span>
      <h1>Preparing document review</h1>
      <p>Loading the static preview and checking the request.</p>
    </section>
  </main>

  <main v-else-if="phase === 'error'" class="app-state-screen">
    <section class="app-state-card" role="alert">
      <p class="product-name">Inkwell Desktop</p>
      <p class="state-code">Review unavailable</p>
      <h1>Something went wrong</h1>
      <p>{{ loadError }}</p>
      <button type="button" @click="loadReview()">Try again</button>
    </section>
  </main>

  <SigningRequestScreen
    v-else-if="phase === 'active' && review"
    :review="review"
    @cancelled="
      invalidateReview({
        payload: { requestId: review.requestId },
      } as RequestEvent)
    "
    @continued="loadReview()"
  />
  <WaitingScreen v-else />
</template>
