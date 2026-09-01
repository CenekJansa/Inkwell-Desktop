<script setup lang="ts">
import { computed, nextTick, ref, useTemplateRef } from "vue";

import { cancelSigningRequest, continueSigningRequest } from "../api/pdfReview";
import PdfReviewPage from "../components/PdfReviewPage.vue";
import type { PdfReview } from "../types/pdfReview";

const MIN_ZOOM = 50;
const MAX_ZOOM = 200;
const ZOOM_STEP = 25;

const props = defineProps<{ review: PdfReview }>();
const emit = defineEmits<{ cancelled: []; continued: [] }>();
const zoom = ref(100);
const selectedPage = ref(props.review.pages[0]?.pageNumber ?? 1);
const action = ref<"cancel" | "continue" | null>(null);
const actionError = ref("");
const pageStack = useTemplateRef("pageStack");

const pageCount = computed(() => props.review.pages.length);
const canContinue = computed(
  () => props.review.status === "ready" && pageCount.value > 0 && !action.value,
);

function formatSize(bytes: number) {
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function changeZoom(amount: number) {
  zoom.value = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom.value + amount));
}

async function goToPage(pageNumber: number) {
  selectedPage.value = pageNumber;
  await nextTick();
  pageStack.value
    ?.querySelector(`[data-page-number="${pageNumber}"]`)
    ?.scrollIntoView({ behavior: "smooth", block: "start" });
}

async function cancelRequest() {
  if (action.value) return;
  action.value = "cancel";
  actionError.value = "";
  try {
    await cancelSigningRequest(props.review.requestId);
    emit("cancelled");
  } catch {
    action.value = null;
    actionError.value = "The request could not be cancelled. Please try again.";
  }
}

async function continueReview() {
  if (!canContinue.value) return;
  action.value = "continue";
  actionError.value = "";
  try {
    await continueSigningRequest(props.review.requestId);
    emit("continued");
  } catch {
    action.value = null;
    actionError.value =
      "Inkwell could not continue. The document remains unsigned.";
  }
}
</script>

<template>
  <main class="review-screen">
    <header class="review-header">
      <div>
        <p class="product-name">Inkwell Desktop</p>
        <p class="review-step">01 / Document review</p>
      </div>
      <div class="document-heading">
        <h1>{{ review.documentName }}</h1>
        <p>
          {{ pageCount }} {{ pageCount === 1 ? "page" : "pages" }} &middot;
          {{ formatSize(review.documentSizeBytes) }} &middot;
          {{ review.websiteOrigin }}
        </p>
      </div>
      <div class="security-mark" aria-label="Static, local preview">
        <span aria-hidden="true">I</span>
        Static preview
      </div>
    </header>

    <section class="review-workspace" aria-label="Document preview">
      <aside class="review-sidebar">
        <div class="metadata-block">
          <p class="eyebrow">Document</p>
          <dl>
            <div>
              <dt>Pages</dt>
              <dd>{{ pageCount }}</dd>
            </div>
            <div>
              <dt>Size</dt>
              <dd>{{ formatSize(review.documentSizeBytes) }}</dd>
            </div>
            <div>
              <dt>Requested by</dt>
              <dd>{{ review.websiteOrigin }}</dd>
            </div>
          </dl>
        </div>

        <nav class="page-navigation" aria-label="Page navigation">
          <p class="eyebrow">Go to page</p>
          <div class="page-select-row">
            <button
              type="button"
              class="icon-button"
              aria-label="Previous page"
              :disabled="selectedPage === review.pages[0]?.pageNumber"
              @click="goToPage(selectedPage - 1)"
            >
              &uarr;
            </button>
            <label>
              <span class="sr-only">Page number</span>
              <select
                :value="selectedPage"
                @change="
                  goToPage(Number(($event.target as HTMLSelectElement).value))
                "
              >
                <option
                  v-for="page in review.pages"
                  :key="page.pageNumber"
                  :value="page.pageNumber"
                >
                  {{ page.pageNumber }} / {{ pageCount }}
                </option>
              </select>
            </label>
            <button
              type="button"
              class="icon-button"
              aria-label="Next page"
              :disabled="selectedPage === review.pages.at(-1)?.pageNumber"
              @click="goToPage(selectedPage + 1)"
            >
              &darr;
            </button>
          </div>
        </nav>

        <p class="preview-note">
          Pages are rendered as static images. Links, forms, scripts, and
          attachments are disabled.
        </p>
      </aside>

      <div class="preview-column">
        <div class="viewer-toolbar" aria-label="Preview controls">
          <span>Review every page before continuing</span>
          <div class="zoom-controls">
            <button
              type="button"
              class="icon-button"
              aria-label="Zoom out"
              :disabled="zoom === MIN_ZOOM"
              @click="changeZoom(-ZOOM_STEP)"
            >
              &minus;
            </button>
            <output aria-live="polite">{{ zoom }}%</output>
            <button
              type="button"
              class="icon-button"
              aria-label="Zoom in"
              :disabled="zoom === MAX_ZOOM"
              @click="changeZoom(ZOOM_STEP)"
            >
              +
            </button>
          </div>
        </div>

        <div ref="pageStack" class="page-stack" tabindex="0">
          <PdfReviewPage
            v-for="page in review.pages"
            :key="page.pageNumber"
            :request-id="review.requestId"
            :page="page"
            :zoom="zoom"
          />
          <div v-if="pageCount === 0" class="empty-preview" role="alert">
            This document has no pages available to review.
          </div>
        </div>
      </div>
    </section>

    <footer class="review-actions">
      <div class="readiness" :class="`status-${review.status}`" role="status">
        <span aria-hidden="true"></span>
        <p>
          <strong>{{
            review.status === "ready"
              ? "Ready to continue"
              : review.status === "failed"
                ? "Review unavailable"
                : "Checking document"
          }}</strong>
          {{
            review.statusMessage ||
            (review.status === "ready"
              ? "The complete preview is available."
              : review.status === "failed"
                ? "The document could not be prepared."
                : "Continue will unlock when backend checks finish.")
          }}
        </p>
      </div>
      <p v-if="actionError" class="action-error" role="alert">
        {{ actionError }}
      </p>
      <div class="action-buttons">
        <button
          type="button"
          class="secondary-button"
          :disabled="Boolean(action)"
          @click="cancelRequest"
        >
          {{ action === "cancel" ? "Cancelling..." : "Cancel" }}
        </button>
        <button type="button" :disabled="!canContinue" @click="continueReview">
          {{ action === "continue" ? "Continuing..." : "Continue" }}
        </button>
      </div>
    </footer>
  </main>
</template>
