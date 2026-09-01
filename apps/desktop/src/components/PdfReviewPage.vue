<script setup lang="ts">
import { onUnmounted, ref, watch } from "vue";

import { renderPdfReviewPage } from "../api/pdfReview";
import type { PdfPageMetadata } from "../types/pdfReview";

const props = defineProps<{
  requestId: string;
  page: PdfPageMetadata;
  zoom: number;
}>();

const imageUrl = ref<string | null>(null);
const state = ref<"loading" | "loaded" | "error">("loading");
let loadSequence = 0;

function clearImage() {
  if (imageUrl.value) globalThis.URL.revokeObjectURL(imageUrl.value);
  imageUrl.value = null;
}

async function loadPage() {
  const sequence = ++loadSequence;
  clearImage();
  state.value = "loading";

  try {
    const rendered = await renderPdfReviewPage(
      props.requestId,
      props.page.pageNumber,
      props.zoom / 100,
    );
    const bytes =
      rendered instanceof globalThis.ArrayBuffer
        ? new Uint8Array(rendered)
        : new Uint8Array(rendered);
    const url = globalThis.URL.createObjectURL(
      new globalThis.Blob([bytes], { type: "image/png" }),
    );

    if (sequence !== loadSequence) {
      globalThis.URL.revokeObjectURL(url);
      return;
    }

    imageUrl.value = url;
    state.value = "loaded";
  } catch {
    if (sequence === loadSequence) state.value = "error";
  }
}

watch(() => [props.requestId, props.page.pageNumber, props.zoom], loadPage, {
  immediate: true,
});

onUnmounted(() => {
  loadSequence += 1;
  clearImage();
});
</script>

<template>
  <article
    class="pdf-page"
    :data-page-number="page.pageNumber"
    :style="{ aspectRatio: `${page.width} / ${page.height}` }"
    :aria-label="`Page ${page.pageNumber}`"
  >
    <div v-if="state === 'loading'" class="page-state" role="status">
      <span class="page-spinner" aria-hidden="true"></span>
      Loading page {{ page.pageNumber }}
    </div>
    <div
      v-else-if="state === 'error'"
      class="page-state page-error"
      role="alert"
    >
      <strong>Page {{ page.pageNumber }} could not be displayed</strong>
      <button type="button" class="text-button" @click="loadPage">
        Try again
      </button>
    </div>
    <img
      v-else
      :src="imageUrl ?? undefined"
      :alt="`Static preview of page ${page.pageNumber}`"
      draggable="false"
    />
    <span class="page-number" aria-hidden="true">{{ page.pageNumber }}</span>
  </article>
</template>
