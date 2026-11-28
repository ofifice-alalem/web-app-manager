<script setup lang="ts">
import recoveryIllustration from '../assets/recovery-illustration.jpg';

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
    dir="rtl"
    class="relative flex min-h-screen w-full items-center justify-center overflow-hidden bg-white text-slate-800 select-none p-6 lg:p-12"
  >
    <div class="w-full max-w-7xl mx-auto grid grid-cols-1 lg:grid-cols-12 gap-8 lg:gap-12 items-center">
      
      <!-- Right Side (RTL - 5 Cols): Text & Retry Action -->
      <div class="space-y-7 text-right lg:col-span-5 order-2 lg:order-1">
        
        <!-- Status Pill -->
        <div class="inline-flex items-center gap-2 rounded-full bg-rose-50 px-4 py-2 text-xs font-black text-rose-700 border border-rose-200/60 shadow-sm">
          <span class="inline-block h-2.5 w-2.5 rounded-full bg-rose-500"></span>
          <span>تعذر الاتصال بالمنظومة المحلية</span>
        </div>

        <!-- Headline -->
        <div class="space-y-3">
          <h1 class="text-4xl lg:text-6xl font-black text-slate-900 tracking-tight leading-tight">
            <span class="text-rose-600">عذراً!</span>
            الخدمة غير متاحة حالياً
          </h1>
          <p class="text-slate-500 text-base lg:text-lg font-medium leading-relaxed max-w-xl">
            {{ message || 'تعذر الوصول إلى المنظومة أو تشغيل الخدمات المطلوبة. يمكنك الضغط على زر إعادة المحاولة للتحقق مجدداً.' }}
          </p>
        </div>

        <!-- Retry Action -->
        <div class="pt-4">
          <button
            type="button"
            class="inline-flex items-center justify-center gap-2.5 rounded-xl bg-gradient-to-r from-[#0284c7] to-[#0ea5e9] px-8 py-3.5 text-sm font-extrabold text-white shadow-xl shadow-sky-500/25 transition-all hover:brightness-110 active:scale-95 focus:outline-none focus:ring-2 focus:ring-sky-400"
            @click="emit('retry')"
          >
            <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"></path>
            </svg>
            إعادة المحاولة الآن
          </button>
        </div>

      </div>

      <!-- Left Side (RTL - 7 Cols): Large Recovery Illustration (Seamless Blend, No Shadow) -->
      <div class="lg:col-span-7 relative flex items-center justify-center order-1 lg:order-2 bg-transparent select-none">
        <div class="relative w-[480px] h-[480px] sm:w-[620px] sm:h-[620px] flex items-center justify-center bg-transparent">
          <img
            :src="recoveryIllustration"
            alt="Taqniya System Recovery"
            class="w-full h-full object-contain blend-clean select-none pointer-events-none drop-shadow-none"
          />
        </div>
      </div>

    </div>
  </div>
</template>

<style scoped>
.blend-clean {
  mix-blend-mode: multiply;
}
</style>
