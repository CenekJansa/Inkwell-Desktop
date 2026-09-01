<script setup lang="ts">
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { onMounted, onUnmounted, ref } from "vue";

import { getPendingSigningRequest } from "./api/walkingSkeleton";
import WaitingScreen from "./screens/WaitingScreen.vue";
import SigningRequestScreen from "./screens/SigningRequestScreen.vue";
import type { SigningRequestDisplay } from "./types/signingRequest";

const request = ref<SigningRequestDisplay | null>(null);
let unlisten: UnlistenFn | undefined;

async function loadRequest() {
  try {
    request.value = await getPendingSigningRequest();
  } catch {
    request.value = null;
  }
}

onMounted(async () => {
  unlisten = await listen("signing-request-available", loadRequest);
  await loadRequest();
});

onUnmounted(() => unlisten?.());
</script>

<template>
  <SigningRequestScreen
    v-if="request"
    :request="request"
    @cancelled="request = null"
  />
  <WaitingScreen v-else />
</template>
