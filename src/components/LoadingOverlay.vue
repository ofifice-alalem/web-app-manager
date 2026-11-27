<script setup lang="ts">
defineProps<{
  message: string;
}>();
</script>

<template>
  <div
    role="status"
    aria-live="polite"
    class="flex min-h-screen flex-col items-center justify-center bg-slate-50 px-6 text-center"
  >
    <div
      class="h-10 w-10 animate-spin rounded-full border-4 border-slate-200 border-t-slate-700"
      aria-hidden="true"
    />
    <p class="mt-4 text-sm font-medium text-slate-700">
      {{ message }}
    </p>
  </div>
</template>
