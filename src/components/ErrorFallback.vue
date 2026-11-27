<script setup lang="ts">
defineProps<{
  message: string;
}>();

const emit = defineEmits<{
  retry: [];
}>();
</script>

<template>
  <div
    role="alert"
    class="flex min-h-screen flex-col items-center justify-center bg-slate-50 px-6 text-center"
  >
    <div class="max-w-md rounded-xl border border-slate-200 bg-white p-8 shadow-sm">
      <div
        class="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-full bg-rose-50 text-rose-600"
        aria-hidden="true"
      >
        <svg class="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            d="M12 9v3.75m9-.75a9 9 0 1 1-18 0 9 9 0 0 1 18 0Zm-9 3.75h.008v.008H12v-.008Z"
          />
        </svg>
      </div>
      <h1 class="text-lg font-semibold text-slate-900">Application Unavailable</h1>
      <p class="mt-2 text-sm leading-relaxed text-slate-600">
        {{ message }}
      </p>
      <button
        type="button"
        class="mt-6 inline-flex items-center justify-center rounded-lg bg-slate-900 px-4 py-2 text-sm font-medium text-white transition hover:bg-slate-800 focus:outline-none focus:ring-2 focus:ring-slate-500 focus:ring-offset-2"
        @click="emit('retry')"
      >
        Retry
      </button>
    </div>
  </div>
</template>
