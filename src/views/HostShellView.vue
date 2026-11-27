<script setup lang="ts">
import LoadingOverlay from '../components/LoadingOverlay.vue';
import ErrorFallback from '../components/ErrorFallback.vue';
import { useShellState } from '../composables/useShellState';

const { status, isLoading, isUnavailable, retryConnection } = useShellState();
</script>

<template>
  <main class="min-h-screen w-full bg-slate-50">
    <LoadingOverlay v-if="isLoading" :message="status.message" />
    <ErrorFallback
      v-else-if="isUnavailable"
      :message="status.message"
      @retry="retryConnection"
    />
  </main>
</template>
