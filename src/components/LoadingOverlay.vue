<script setup lang="ts">
import companyLogo from '../assets/logo.ico';

defineProps<{
  message: string;
}>();
</script>

<template>
  <div
    role="status"
    aria-live="polite"
    dir="rtl"
    class="relative flex min-h-screen items-center justify-center overflow-hidden bg-white px-8 py-12 text-slate-900"
  >
    <div class="mx-auto grid w-full max-w-5xl grid-cols-1 items-center gap-12 lg:grid-cols-2">
      <!-- Right Column (RTL): Logo + Headline + Message + Smooth Progress Bar -->
      <div class="space-y-6 text-right">
        <div class="inline-flex items-center gap-3">
          <img :src="companyLogo" alt="Company Logo" class="h-14 w-14 object-contain" />
        </div>

        <h1 class="text-4xl font-extrabold leading-tight text-slate-900 sm:text-5xl">
          <span class="text-[#3730a3]">لحظة!</span>
          جارٍ تجهيز المنظومة
        </h1>

        <p class="max-w-md text-base font-medium leading-relaxed text-slate-500">
          {{ message }}
        </p>

        <!-- Smooth Sliding Progress Bar -->
        <div class="max-w-sm pt-2">
          <div class="h-2 w-full overflow-hidden rounded-full bg-slate-100">
            <div class="animate-steam-progress h-full w-1/2 rounded-full bg-gradient-to-r from-[#38bdf8] via-[#0ea5e9] to-[#3730a3]" />
          </div>
        </div>
      </div>

      <!-- Left Column (RTL): Watermark + Blue Cable Illustration -->
      <div class="relative flex items-center justify-center select-none" aria-hidden="true">
        <span class="text-[9rem] font-black tracking-tighter text-slate-100 sm:text-[11rem]">
          503
        </span>

        <svg
          class="absolute h-64 w-64 drop-shadow-xl sm:h-72 sm:w-72"
          viewBox="0 0 260 220"
          fill="none"
          xmlns="http://www.w3.org/2000/svg"
        >
          <path
            d="M15 95 C 95 95, 115 45, 165 65 C 210 82, 185 155, 115 162"
            stroke="#0284c7"
            stroke-width="11"
            stroke-linecap="round"
          />
          <path
            d="M15 95 C 95 95, 115 45, 165 65 C 210 82, 185 155, 115 162"
            stroke="#38bdf8"
            stroke-width="4"
            stroke-linecap="round"
          />
          <g transform="translate(68, 140) rotate(-12)">
            <rect x="0" y="6" width="46" height="32" rx="7" fill="#0284c7" />
            <rect x="6" y="10" width="34" height="10" rx="3" fill="#38bdf8" opacity="0.5" />
            <rect x="44" y="12" width="10" height="20" rx="2" fill="#0369a1" />
            <rect x="-16" y="13" width="16" height="5" rx="2" fill="#94a3b8" />
            <rect x="-16" y="26" width="16" height="5" rx="2" fill="#94a3b8" />
          </g>
        </svg>
      </div>
    </div>
  </div>
</template>
