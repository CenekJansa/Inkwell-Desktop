<script setup lang="ts">
import { onMounted, ref } from "vue";

import { getPendingSigningRequest } from "./api/walkingSkeleton";
import WaitingScreen from "./screens/WaitingScreen.vue";
import SigningRequestScreen from "./screens/SigningRequestScreen.vue";
import type { SigningRequestDisplay } from "./types/signingRequest";

const request = ref<SigningRequestDisplay | null>(null);

onMounted(async () => {
  try {
    request.value = await getPendingSigningRequest();
  } catch {
    request.value = null;
  }
});
</script>

<template>
  <SigningRequestScreen
    v-if="request"
    :request="request"
    @cancelled="request = null"
  />
  <WaitingScreen v-else />
</template>
