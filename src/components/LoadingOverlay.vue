<script setup lang="ts">
import recoveryIllustration from '../assets/recovery-illustration.jpg';

defineProps<{
  message: string;
}>();
</script>

<template>
  <div
    role="status"
    aria-live="polite"
    dir="rtl"
    class="relative flex min-h-screen w-full items-center justify-center overflow-hidden bg-white text-slate-800 select-none p-6 lg:p-12"
  >
    <div class="w-full max-w-7xl mx-auto grid grid-cols-1 lg:grid-cols-12 gap-8 lg:gap-12 items-center">
      
      <!-- Right Side (RTL - 5 Cols): Text & Animated Progress -->
      <div class="lg:col-span-5 space-y-7 text-right order-2 lg:order-1">
        
        <!-- Modern Tech Pill -->
        <div class="inline-flex items-center gap-2.5 px-4 py-2 rounded-full bg-blue-50/90 border border-blue-200/60 text-[#1e40af] text-xs font-black shadow-sm">
          <span class="relative flex h-2.5 w-2.5">
            <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-sky-400 opacity-75"></span>
            <span class="relative inline-flex rounded-full h-2.5 w-2.5 bg-[#1d4ed8]"></span>
          </span>
          <span>الاتصال بالخادم قيد المعالجة...</span>
        </div>

        <!-- Headline -->
        <div class="space-y-3">
          <h1 class="text-4xl lg:text-6xl font-black text-slate-900 tracking-tight leading-tight">
            <span class="text-[#1d4ed8]">لحظة!</span>
            جـارٍ تجهيز المنظومة
          </h1>
          <p class="text-slate-500 text-base lg:text-lg font-medium leading-relaxed max-w-xl">
            {{ message || 'نقوم الآن بالتحقق من استجابة قاعدة البيانات ومحرك الخدمات محلياً، وسيتم تحويلك لواجهة البيع فور اكتمال الجاهزية.' }}
          </p>
        </div>

        <!-- Progress Bar Box -->
        <div class="pt-4 space-y-3 max-w-lg">
          <div class="flex justify-between items-center text-xs font-bold text-slate-400 px-1">
            <span>استعادة الخدمة وإعادة التحميل الذاتي</span>
            <span class="font-mono text-[#1d4ed8] font-black">الخطوة 1 من 2</span>
          </div>
          
          <div class="h-3.5 w-full bg-slate-100 rounded-full overflow-hidden shadow-inner p-0.5 border border-slate-200/50">
            <div class="h-full w-2/5 bg-gradient-to-l from-[#1d4ed8] via-[#0284c7] to-[#38bdf8] rounded-full animate-steam-progress"></div>
          </div>
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

@keyframes steamProgress {
  0% { transform: translateX(180%); }
  50% { transform: translateX(0%); }
  100% { transform: translateX(-180%); }
}

.animate-steam-progress {
  animation: steamProgress 2.4s cubic-bezier(0.4, 0, 0.2, 1) infinite;
}
</style>
