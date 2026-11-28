<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import companyLogo from './assets/logo.ico';

type Tab = 'setup' | 'logs';
type SimpleErrorKind = 'connection_refused' | 'http_status' | 'timeout';

interface CommandStepForm {
  commandText: string;
  waitSeconds: number;
}

interface SimpleRuleForm {
  errorKind: SimpleErrorKind;
  httpCodesText: string;
  steps: CommandStepForm[];
}

interface LogEvent {
  id: number;
  eventType: string;
  ruleId: string | null;
  serviceId: string | null;
  triggerType: string;
  triggerDetails: string;
  status: string;
  attemptCount: number;
  webviewReloaded: boolean;
  startedAt: string;
  finishedAt: string | null;
}

interface CommandExecution {
  id: number;
  executable: string;
  arguments: string;
  stdout: string;
  stderr: string;
  exitCode: number | null;
  status: string;
  durationMs: number;
  executedAt: string;
}

const activeTab = ref<Tab>('setup');
const configDir = ref('');
const feedback = ref<{ type: 'ok' | 'err'; text: string } | null>(null);
const isSaving = ref(false);

const targetUrl = ref('http://127.0.0.1:8000');
const startupTimeoutSeconds = ref(5);
const rules = ref<SimpleRuleForm[]>([
  {
    errorKind: 'connection_refused',
    httpCodesText: '500',
    steps: [
      { commandText: 'docker start perfumes_db perfumes_app', waitSeconds: 3 },
    ],
  },
]);

const logs = ref<LogEvent[]>([]);
const selectedEventId = ref<number | null>(null);
const executions = ref<CommandExecution[]>([]);

function parseCommandLine(cmdLine: string): { executable: string; args: string[] } | null {
  const parts = cmdLine.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return null;
  let executable = parts[0];
  if (executable.toLowerCase() === 'docker') {
    executable = 'docker.exe';
  }
  return {
    executable,
    args: parts.slice(1),
  };
}

function addRule() {
  rules.value.push({
    errorKind: 'http_status',
    httpCodesText: '500, 502, 503',
    steps: [{ commandText: 'docker start perfumes_db perfumes_app', waitSeconds: 5 }],
  });
}

function removeRule(idx: number) {
  if (rules.value.length > 1) {
    rules.value.splice(idx, 1);
  }
}

function addStep(rule: SimpleRuleForm) {
  rule.steps.push({ commandText: '', waitSeconds: 3 });
}

function removeStep(rule: SimpleRuleForm, idx: number) {
  if (rule.steps.length > 1) {
    rule.steps.splice(idx, 1);
  }
}

async function loadAll() {
  feedback.value = null;
  try {
    const bundle = await invoke<{
      configDir: string;
      appJson: string;
      servicesJson: string;
      rulesJson: string;
    }>('load_settings_bundle');
    configDir.value = bundle.configDir;

    const parsedApp = JSON.parse(bundle.appJson);
    if (parsedApp.application?.url) {
      targetUrl.value = parsedApp.application.url;
    }
    if (parsedApp.application?.startup_timeout_seconds) {
      startupTimeoutSeconds.value = Number(parsedApp.application.startup_timeout_seconds) || 5;
    }

    const parsedRules = JSON.parse(bundle.rulesJson);
    if (Array.isArray(parsedRules.rules) && parsedRules.rules.length > 0) {
      rules.value = parsedRules.rules.map((r: any): SimpleRuleForm => {
        const tr = r.trigger || {};
        let errorKind: SimpleErrorKind = 'connection_refused';
        let httpCodesText = '500';
        if (tr.type === 'http_status') {
          errorKind = 'http_status';
          httpCodesText = Array.isArray(tr.status_codes) ? tr.status_codes.join(', ') : '500';
        } else if (tr.type === 'service_status' && tr.value === 'timeout') {
          errorKind = 'timeout';
        }

        const steps: CommandStepForm[] = [];
        const actions = Array.isArray(r.actions) ? r.actions : [];
        for (let i = 0; i < actions.length; i++) {
          const act = actions[i];
          if (act.type === 'command') {
            const exe = act.platform_commands?.linux?.executable || act.platform_commands?.windows?.executable || act.executable || '';
            const args = act.platform_commands?.linux?.args || act.platform_commands?.windows?.args || act.args || [];
            const nextAct = actions[i + 1];
            const waitSeconds = nextAct?.type === 'wait' ? Number(nextAct.seconds) || 3 : 3;
            steps.push({
              commandText: [exe.replace(/\.exe$/i, ''), ...args].filter(Boolean).join(' '),
              waitSeconds,
            });
          }
        }

        if (steps.length === 0) {
          steps.push({ commandText: 'docker start perfumes_db perfumes_app', waitSeconds: 3 });
        }

        return { errorKind, httpCodesText, steps };
      });
    }
  } catch (err) {
    feedback.value = {
      type: 'err',
      text: `فشل قراءة ملفات الإعدادات: ${String(err)}`,
    };
  }
}

async function refreshLogs() {
  try {
    const raw = await invoke<LogEvent[]>('load_recent_logs', { limit: 40 });
    logs.value = raw;
    if (raw.length > 0 && selectedEventId.value === null) {
      selectLog(raw[0].id);
    }
  } catch (err) {
    feedback.value = {
      type: 'err',
      text: `فشل تحميل السجلات: ${String(err)}`,
    };
  }
}

async function selectLog(eventId: number) {
  selectedEventId.value = eventId;
  try {
    executions.value = await invoke<CommandExecution[]>('load_event_executions', { eventId });
  } catch (err) {
    executions.value = [];
  }
}

async function saveAllConfigs() {
  feedback.value = null;
  isSaving.value = true;
  try {
    const cleanUrl = targetUrl.value.trim();
    if (!cleanUrl.startsWith('http://') && !cleanUrl.startsWith('https://')) {
      throw new Error('يجب أن يبدأ الرابط بـ http:// أو https://');
    }

    const appPayload = {
      application: {
        id: 'web-sales-app',
        name: 'نظام المبيعات',
        url: cleanUrl,
        startup_timeout_seconds: Number(startupTimeoutSeconds.value) || 5,
        reload_after_recovery: true,
      },
    };

    const servicesPayload = {
      services: [
        {
          id: 'main-app',
          name: 'الخدمة الأساسية',
          required: true,
          check: {
            type: 'http',
            url: cleanUrl,
            expected_status: [200, 301, 302],
            timeout_seconds: 5,
          },
          commands: {
            start: null,
            stop: null,
            restart: null,
          },
        },
      ],
    };

    const generatedRules = rules.value.map((rule, idx) => {
      let trigger: Record<string, unknown>;
      if (rule.errorKind === 'http_status') {
        const codes = rule.httpCodesText
          .split(',')
          .map((n) => Number(n.trim()))
          .filter((n) => !Number.isNaN(n) && n >= 100 && n <= 599);
        trigger = {
          type: 'http_status',
          service_id: 'main-app',
          status_codes: codes.length ? codes : [500],
        };
      } else if (rule.errorKind === 'timeout') {
        trigger = {
          type: 'service_status',
          service_id: 'main-app',
          value: 'timeout',
        };
      } else {
        trigger = {
          type: 'connection_refused',
          service_id: 'main-app',
        };
      }

      const actions: Record<string, unknown>[] = [];
      const validSteps = rule.steps.filter((s) => s.commandText.trim().length > 0);
      validSteps.forEach((step, sIdx) => {
        const parsed = parseCommandLine(step.commandText);
        if (!parsed) return;
        const winExe = parsed.executable;
        const linuxExe = winExe.replace(/\.exe$/i, '');
        actions.push({
          type: 'command',
          platform_commands: {
            windows: { executable: winExe, args: parsed.args, timeout_seconds: 60 },
            linux: { executable: linuxExe, args: parsed.args, timeout_seconds: 60 },
          },
          timeout_seconds: 60,
          continue_on_failure: sIdx < validSteps.length - 1,
        });
        if (step.waitSeconds > 0) {
          actions.push({
            type: 'wait',
            seconds: Number(step.waitSeconds),
          });
        }
      });

      actions.push({
        type: 'reload_application',
        enabled: true,
      });

      return {
        id: `auto-rule-${idx + 1}-${rule.errorKind}`,
        name: `إصلاح تلقائي: ${rule.errorKind}`,
        enabled: true,
        trigger,
        actions,
        retry: {
          max_attempts: 3,
          delay_seconds: 3,
        },
        cooldown_seconds: 30,
      };
    });

    const rulesPayload = {
      rules: generatedRules,
    };

    await invoke('save_config_file', {
      filename: 'app.json',
      content: JSON.stringify(appPayload, null, 2),
    });
    await invoke('save_config_file', {
      filename: 'services.json',
      content: JSON.stringify(servicesPayload, null, 2),
    });
    await invoke('save_config_file', {
      filename: 'recovery-rules.json',
      content: JSON.stringify(rulesPayload, null, 2),
    });

    feedback.value = {
      type: 'ok',
      text: 'تم حفظ كافة الإعدادات بنجاح في ملفات النظام.',
    };
  } catch (err) {
    feedback.value = {
      type: 'err',
      text: `فشل الحفظ: ${String(err)}`,
    };
  } finally {
    isSaving.value = false;
  }
}

onMounted(() => {
  loadAll();
});
</script>

<template>
  <div dir="rtl" class="cinnamon-app flex h-screen flex-col overflow-hidden text-[#e8e8e8] font-sans select-none bg-[#1e2022]">
    
    <!-- Top HeaderBar (Cinnamon / Mint Style) -->
    <header class="cinnamon-headerbar shrink-0 flex items-center justify-between px-6 py-3 border-b border-[#323639] bg-[#272a2d] shadow-md z-10">
      
      <!-- App Brand -->
      <div class="flex items-center gap-3">
        <div class="app-icon-badge flex h-9 w-9 items-center justify-center rounded-xl bg-[#1e2022] border border-[#3e4347] shadow-inner p-1.5">
          <img :src="companyLogo" alt="Logo" class="h-6 w-6 object-contain" />
        </div>
        <div>
          <h1 class="text-sm font-bold text-white tracking-wide">مدير المنظومة ومضيف التطبيق</h1>
          <p class="text-[11px] text-[#9a9fa5]">إعدادات الاتصال، الفحص، والاستعادة الذاتية</p>
        </div>
      </div>

      <!-- Centered Tab Switcher (Mint Linked Buttons) -->
      <div class="mint-tab-switcher flex rounded-lg bg-[#1a1c1d] border border-[#3a3f43] p-1 shadow-inner">
        <button
          class="tab-btn px-5 py-1.5 text-xs font-semibold rounded-md transition-all flex items-center gap-2"
          :class="activeTab === 'setup' 
            ? 'bg-[#3b82f6] text-white shadow-sm' 
            : 'text-[#9ca3af] hover:text-white hover:bg-[#282b2e]'"
          @click="activeTab = 'setup'"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4"></path>
          </svg>
          قواعد الاستعادة والربط
        </button>
        <button
          class="tab-btn px-5 py-1.5 text-xs font-semibold rounded-md transition-all flex items-center gap-2"
          :class="activeTab === 'logs' 
            ? 'bg-[#3b82f6] text-white shadow-sm' 
            : 'text-[#9ca3af] hover:text-white hover:bg-[#282b2e]'"
          @click="activeTab = 'logs'; refreshLogs()"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"></path>
          </svg>
          سجل عمليات الإصلاح
        </button>
      </div>

      <!-- Action Button -->
      <div>
        <button
          :disabled="isSaving"
          class="mint-btn-primary px-5 py-1.5 text-xs font-bold rounded-lg bg-[#22c55e] hover:bg-[#16a34a] active:bg-[#15803d] text-white shadow-md shadow-green-900/20 border border-[#16a34a] transition-all disabled:opacity-50 flex items-center gap-2"
          @click="saveAllConfigs"
        >
          <svg v-if="!isSaving" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path>
          </svg>
          {{ isSaving ? 'جارٍ الحفظ...' : 'حفظ الإعدادات' }}
        </button>
      </div>
    </header>

    <!-- Main Content Area -->
    <main class="flex-1 overflow-y-auto px-6 py-6 bg-[#181a1b]">
      <div class="max-w-4xl mx-auto space-y-6">
        
        <!-- Feedback Alert -->
        <div
          v-if="feedback"
          class="mint-alert flex items-center justify-between rounded-xl px-4 py-3 text-xs font-medium border shadow-sm"
          :class="feedback.type === 'ok'
            ? 'border-emerald-600/40 bg-emerald-950/40 text-emerald-300'
            : 'border-rose-600/40 bg-rose-950/40 text-rose-300'"
        >
          <div class="flex items-center gap-2.5">
            <span class="inline-block w-2.5 h-2.5 rounded-full" :class="feedback.type === 'ok' ? 'bg-emerald-400' : 'bg-rose-400'"></span>
            <span class="text-xs">{{ feedback.text }}</span>
          </div>
          <button class="text-xs opacity-70 hover:opacity-100 px-2 py-0.5 rounded" @click="feedback = null">✕</button>
        </div>

        <template v-if="activeTab === 'setup'">
          
          <!-- Section 1: Server URL & Startup -->
          <div class="mint-section space-y-2.5">
            <div class="flex items-center gap-2 px-1">
              <span class="text-[12px] font-bold text-[#8fa0b0] uppercase tracking-wider">الربط الأساسي والمراقبة</span>
            </div>
            
            <div class="mint-card rounded-2xl bg-[#24272a] border border-[#34393e] shadow-md p-5 space-y-4">
              <div class="grid grid-cols-1 md:grid-cols-4 gap-4 items-center">
                
                <!-- URL Field -->
                <div class="md:col-span-3 space-y-1.5">
                  <div class="flex items-center justify-between">
                    <label class="text-xs font-bold text-white">رابط المنظومة الأساسي (Target URL)</label>
                    <span class="text-[11px] text-[#8e969d]">عنوان الويب المستضاف محلياً أو على الشبكة</span>
                  </div>
                  <div class="relative">
                    <input
                      v-model="targetUrl"
                      dir="ltr"
                      type="url"
                      placeholder="http://127.0.0.1:8000"
                      class="mint-input w-full rounded-xl border border-[#3e444a] bg-[#1a1c1e] px-3.5 py-2.5 text-xs font-mono text-[#60a5fa] placeholder-[#555d65] focus:border-[#3b82f6] focus:bg-[#151718] focus:outline-none transition shadow-inner"
                    />
                  </div>
                </div>

                <!-- Timeout Field -->
                <div class="space-y-1.5">
                  <label class="text-xs font-bold text-white block">مهلة الفحص</label>
                  <div class="flex items-center gap-2">
                    <input
                      v-model.number="startupTimeoutSeconds"
                      type="number"
                      min="1"
                      max="180"
                      class="mint-input w-24 rounded-xl border border-[#3e444a] bg-[#1a1c1e] px-3 py-2 text-xs font-semibold text-center text-white focus:border-[#3b82f6] focus:outline-none shadow-inner"
                    />
                    <span class="text-xs text-[#9aa2a9]">ثوانٍ</span>
                  </div>
                </div>

              </div>
            </div>
          </div>

          <!-- Section 2: Automated Recovery Rules -->
          <div class="mint-section space-y-3 pt-2">
            
            <div class="flex items-center justify-between px-1">
              <div>
                <span class="text-[12px] font-bold text-[#8fa0b0] uppercase tracking-wider">حالات الخطأ وأوامر الاستعادة التلقائية</span>
                <p class="text-[11px] text-[#78828c] mt-0.5">يتم فحص الرابط باستمرار، وعند تعثره يتم تشغيل الأوامر بالترتيب لإعادة النظام للعمل</p>
              </div>
              <button
                class="mint-btn-secondary px-3.5 py-1.5 rounded-lg bg-[#2f3438] hover:bg-[#383e43] border border-[#444b52] text-xs font-semibold text-[#60a5fa] hover:text-white transition flex items-center gap-1.5 shadow-sm"
                @click="addRule"
              >
                <span class="text-sm font-bold leading-none">+</span>
                <span>إضافة حالة خطأ</span>
              </button>
            </div>

            <!-- Rules List -->
            <div class="space-y-4">
              <div
                v-for="(rule, rIdx) in rules"
                :key="rIdx"
                class="rule-card rounded-2xl bg-[#24272a] border border-[#34393e] shadow-md p-5 space-y-4 transition-all"
              >
                <!-- Rule Header: Condition & Status -->
                <div class="flex flex-wrap items-center justify-between gap-4 pb-3 border-b border-[#32363b]">
                  
                  <div class="flex items-center gap-3 flex-1 min-w-[280px]">
                    <span class="h-6 w-6 rounded-lg bg-[#3b82f6]/20 text-[#60a5fa] border border-[#3b82f6]/40 flex items-center justify-center text-xs font-bold shrink-0">
                      {{ rIdx + 1 }}
                    </span>
                    
                    <div class="flex-1">
                      <label class="block text-[11px] font-bold text-[#9ca3af] mb-1">نوع العطل / الخطأ المرصود:</label>
                      <div class="relative">
                        <select
                          v-model="rule.errorKind"
                          class="mint-select w-full rounded-xl border border-[#3e444a] bg-[#1a1c1e] text-[#f3f4f6] px-3.5 py-2 text-xs font-medium focus:border-[#3b82f6] focus:outline-none appearance-none cursor-pointer"
                        >
                          <option value="connection_refused" class="bg-[#1a1c1e] text-white">توقف الخدمة / رفض الاتصال (Connection Refused)</option>
                          <option value="http_status" class="bg-[#1a1c1e] text-white">رمز استجابة ويب خاطئ (HTTP 500 / 502 / 503)</option>
                          <option value="timeout" class="bg-[#1a1c1e] text-white">تجاوز مهلة الاستجابة (Connection Timeout)</option>
                        </select>
                        <div class="pointer-events-none absolute inset-y-0 left-0 flex items-center px-3 text-[#9ca3af]">
                          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"></path>
                          </svg>
                        </div>
                      </div>
                    </div>
                  </div>

                  <!-- HTTP Code Box if HTTP error selected -->
                  <div v-if="rule.errorKind === 'http_status'" class="w-40">
                    <label class="block text-[11px] font-bold text-[#9ca3af] mb-1">رموز الأكواد المستهدفة:</label>
                    <input
                      v-model="rule.httpCodesText"
                      dir="ltr"
                      placeholder="500, 502, 503"
                      class="mint-input w-full rounded-xl border border-[#3e444a] bg-[#1a1c1e] px-3 py-2 text-xs font-mono text-[#fbbf24] focus:border-[#3b82f6] focus:outline-none"
                    />
                  </div>

                  <!-- Delete Rule Button -->
                  <div class="self-end pb-0.5">
                    <button
                      v-if="rules.length > 1"
                      class="text-xs font-semibold text-[#f87171] hover:text-white px-3 py-1.5 rounded-lg hover:bg-rose-950/40 border border-transparent hover:border-rose-800/50 transition"
                      @click="removeRule(rIdx)"
                    >
                      حذف الحالة
                    </button>
                  </div>
                </div>

                <!-- Recovery Actions Sequence -->
                <div class="space-y-2.5">
                  <div class="flex items-center justify-between text-xs">
                    <div class="flex items-center gap-1.5 text-[#9ca3af]">
                      <svg class="w-3.5 h-3.5 text-[#60a5fa]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z"></path>
                      </svg>
                      <span class="font-bold text-white">خطوات الإصلاح التسلسلية:</span>
                      <span class="text-[11px] text-[#71717a]">(تُنفذ تباعاً في حال حدوث هذا العطل)</span>
                    </div>

                    <button
                      class="text-xs font-semibold text-[#60a5fa] hover:text-[#93c5fd] hover:underline flex items-center gap-1"
                      @click="addStep(rule)"
                    >
                      <span>+ إضافة أمر تنفيذ</span>
                    </button>
                  </div>

                  <!-- Step Rows -->
                  <div class="space-y-2">
                    <div
                      v-for="(step, sIdx) in rule.steps"
                      :key="sIdx"
                      class="step-row flex flex-wrap items-center gap-3 rounded-xl bg-[#1c1e20] border border-[#2f3337] p-3 shadow-inner"
                    >
                      <!-- Step index -->
                      <span class="flex h-5 w-5 items-center justify-center rounded-md bg-[#2b2f34] text-[11px] font-mono font-bold text-[#9ca3af] shrink-0">
                        {{ sIdx + 1 }}
                      </span>

                      <!-- Command Input -->
                      <div class="flex-1 min-w-[240px]">
                        <input
                          v-model="step.commandText"
                          dir="ltr"
                          placeholder="docker start perfumes_db perfumes_app"
                          class="mint-input w-full rounded-lg border border-[#3b4046] bg-[#141517] px-3 py-1.5 text-xs font-mono text-[#4ade80] placeholder-[#475569] focus:border-[#3b82f6] focus:outline-none"
                        />
                      </div>

                      <!-- Wait Input -->
                      <div class="flex items-center gap-1.5 text-xs text-[#9ca3af]">
                        <span class="text-[11px]">انتظار بعده:</span>
                        <input
                          v-model.number="step.waitSeconds"
                          type="number"
                          min="0"
                          class="mint-input w-14 rounded-lg border border-[#3b4046] bg-[#141517] px-2 py-1 text-center text-xs font-semibold text-white focus:border-[#3b82f6] focus:outline-none"
                        />
                        <span class="text-[11px]">ثوانٍ</span>
                      </div>

                      <!-- Remove Step -->
                      <button
                        v-if="rule.steps.length > 1"
                        class="text-[#9ca3af] hover:text-[#f87171] p-1 rounded hover:bg-[#282b2e] transition"
                        title="حذف هذا الأمر"
                        @click="removeStep(rule, sIdx)"
                      >
                        ✕
                      </button>
                    </div>
                  </div>
                </div>

              </div>
            </div>

          </div>

        </template>

        <!-- Tab 2: Logs Inspector -->
        <template v-else>
          <div class="mint-section space-y-3">
            <div class="flex items-center justify-between px-1">
              <div>
                <span class="text-[12px] font-bold text-[#8fa0b0] uppercase tracking-wider">سجل عمليات الإصلاح والاستعادة</span>
                <p class="text-[11px] text-[#78828c]">مراقبة سجل الأحداث والأوامر التي تم تنفيذها تلقائياً</p>
              </div>
              <button
                class="mint-btn-secondary px-4 py-1.5 rounded-lg bg-[#2f3438] hover:bg-[#383e43] border border-[#444b52] text-xs font-semibold text-[#60a5fa] hover:text-white transition flex items-center gap-1.5"
                @click="refreshLogs"
              >
                <span>تحديث السجل</span>
              </button>
            </div>

            <div v-if="logs.length === 0" class="mint-card rounded-2xl bg-[#24272a] border border-[#34393e] p-12 text-center text-xs text-[#9ca3af] shadow-md">
              لم يتم تسجيل أي عمليات استعادة تلقائية بعد. النظام يعمل بشكل طبيعي.
            </div>

            <div v-else class="grid grid-cols-1 md:grid-cols-2 gap-4">
              
              <!-- Events List -->
              <div class="mint-card rounded-2xl bg-[#24272a] border border-[#34393e] divide-y divide-[#32363b] max-h-[480px] overflow-y-auto shadow-md">
                <div
                  v-for="ev in logs"
                  :key="ev.id"
                  class="p-3.5 cursor-pointer hover:bg-[#2c3034] transition text-xs space-y-1.5"
                  :class="selectedEventId === ev.id ? 'bg-[#3b82f6]/15 border-r-4 border-[#3b82f6]' : ''"
                  @click="selectLog(ev.id)"
                >
                  <div class="flex items-center justify-between">
                    <span class="font-bold text-white">{{ ev.triggerType }}</span>
                    <span
                      class="px-2 py-0.5 rounded-md text-[10px] font-mono font-bold"
                      :class="ev.status === 'succeeded' ? 'bg-emerald-950/60 text-emerald-400 border border-emerald-800/40' : 'bg-rose-950/60 text-rose-400 border border-rose-800/40'"
                    >
                      {{ ev.status }}
                    </span>
                  </div>
                  <div class="text-[11px] font-mono text-[#9ca3af]" dir="ltr">{{ ev.startedAt }}</div>
                </div>
              </div>

              <!-- Executions Details -->
              <div class="mint-card rounded-2xl bg-[#1f2224] border border-[#34393e] p-4 space-y-3 max-h-[480px] overflow-y-auto shadow-md">
                <div class="text-xs font-bold text-white border-b border-[#32363b] pb-2 flex items-center justify-between">
                  <span>تفاصيل تنفيذ الأوامر</span>
                  <span v-if="selectedEventId" class="text-[11px] text-[#9ca3af] font-mono">حدث #{{ selectedEventId }}</span>
                </div>
                
                <div v-if="executions.length === 0" class="text-xs text-[#9ca3af] py-6 text-center">
                  اختر حدثاً من القائمة الجانبية لعرض تفاصيل ومخرجات الأوامر.
                </div>
                
                <div
                  v-for="ex in executions"
                  :key="ex.id"
                  class="rounded-xl bg-[#141517] border border-[#2b2f33] p-3 text-xs space-y-2 font-mono shadow-inner"
                  dir="ltr"
                >
                  <div class="text-[#60a5fa] font-bold">{{ ex.executable }} {{ ex.arguments }}</div>
                  <div class="text-[11px] text-[#9ca3af]">
                    status: <span class="text-white">{{ ex.status }}</span> | exit: {{ ex.exitCode }} | {{ ex.durationMs }}ms
                  </div>
                  <pre v-if="ex.stdout" class="bg-[#0b0c0d] p-2.5 rounded-lg text-[11px] text-[#e2e8f0] overflow-x-auto border border-[#222528]">{{ ex.stdout }}</pre>
                  <pre v-if="ex.stderr" class="bg-[#241013] p-2.5 rounded-lg text-[11px] text-[#fca5a5] overflow-x-auto border border-rose-900/30">{{ ex.stderr }}</pre>
                </div>
              </div>

            </div>
          </div>
        </template>

      </div>
    </main>

    <!-- Bottom Statusbar (Mint Native Feel) -->
    <footer class="shrink-0 px-6 py-2 border-t border-[#323639] bg-[#24272a] text-[11px] text-[#8fa0b0] flex items-center justify-between shadow-inner">
      <div class="flex items-center gap-2">
        <span class="w-2 h-2 rounded-full bg-emerald-400"></span>
        <span>مجلد التهيئة النشط:</span>
        <span class="font-mono text-[#cbd5e1] font-semibold" dir="ltr">{{ configDir || '~/.config/desktop-application-host' }}</span>
      </div>
      <div>نظام التشغيل: Linux Mint (Cinnamon Desktop)</div>
    </footer>

  </div>
</template>

<style scoped>
.cinnamon-app {
  font-family: -apple-system, BlinkMacSystemFont, "Ubuntu", "Segoe UI", "Tajawal", Roboto, sans-serif;
}

/* Custom Scrollbars */
::-webkit-scrollbar {
  width: 6px;
  height: 6px;
}
::-webkit-scrollbar-track {
  background: transparent;
}
::-webkit-scrollbar-thumb {
  background: #3e444a;
  border-radius: 9999px;
}
::-webkit-scrollbar-thumb:hover {
  background: #525a62;
}

/* Input Focus Glow */
.mint-input:focus, .mint-select:focus {
  box-shadow: 0 0 0 2px rgba(59, 130, 246, 0.35);
}
</style>
