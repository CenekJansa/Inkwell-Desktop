<script setup lang="ts">
import { getCurrentWindow } from "@tauri-apps/api/window";

async function closeApplication() {
  await getCurrentWindow().close();
}
</script>

<template>
  <main class="waiting-screen">
    <section class="waiting-card" aria-labelledby="waiting-title">
      <p class="product-name">Inkwell Desktop</p>
      <div class="seal" aria-hidden="true">I</div>
      <h1 id="waiting-title">Start signing from your browser</h1>
      <p class="waiting-copy">
        Inkwell opens automatically when an approved Chrome extension sends a
        document for review. No document is waiting right now.
      </p>
      <button type="button" @click="closeApplication">Close</button>
    </section>
  </main>
</template>
