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
const isDarkMode = ref(true);
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
      { commandText: 'docker restart webapp_container', waitSeconds: 3 },
    ],
  },
]);

const logs = ref<LogEvent[]>([]);
const selectedEventId = ref<number | null>(null);
const executions = ref<CommandExecution[]>([]);

function setTheme(dark: boolean) {
  isDarkMode.value = dark;
  window.localStorage.setItem('settings_theme', dark ? 'dark' : 'light');
}

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
    steps: [{ commandText: 'docker restart webapp_container', waitSeconds: 3 }],
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
          steps.push({ commandText: 'docker restart webapp_container', waitSeconds: 3 });
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
  const saved = localStorage.getItem('settings_theme');
  if (saved) {
    isDarkMode.value = saved === 'dark';
  }
  loadAll();
});
</script>

<template>
  <div
    dir="rtl"
    class="steam-client flex h-screen flex-col overflow-hidden select-none font-sans"
    :class="isDarkMode ? 'dark-theme' : 'light-theme'"
  >
    <!-- Steam Top Window Header -->
    <header class="steam-top-bar flex h-11 shrink-0 items-center justify-between px-4 z-20">
      <div class="flex items-center gap-3">
        <img :src="companyLogo" alt="Logo" class="h-6 w-6 object-contain" />
        <span class="text-xs font-bold uppercase tracking-wider title-text">
          Taqniya Settings // إعدادات تقنية
        </span>
      </div>

      <div class="flex items-center gap-4 text-xs">
        <!-- Professional Theme Segmented Switch -->
        <div class="theme-switch-container flex items-center gap-1 p-0.5 rounded-full border border-subtle">
          <button
            type="button"
            class="theme-pill-btn flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-bold transition"
            :class="!isDarkMode ? 'theme-pill-active' : 'theme-pill-idle'"
            @click="setTheme(false)"
          >
            <svg class="h-3.5 w-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2">
              <circle cx="12" cy="12" r="5"></circle>
              <line x1="12" y1="1" x2="12" y2="3"></line>
              <line x1="12" y1="21" x2="12" y2="23"></line>
              <line x1="4.22" y1="4.22" x2="5.64" y2="5.64"></line>
              <line x1="18.36" y1="18.36" x2="19.78" y2="19.78"></line>
              <line x1="1" y1="12" x2="3" y2="12"></line>
              <line x1="21" y1="12" x2="23" y2="12"></line>
              <line x1="4.22" y1="19.78" x2="5.64" y2="18.36"></line>
              <line x1="18.36" y1="5.64" x2="19.78" y2="4.22"></line>
            </svg>
            <span>فاتح</span>
          </button>

          <button
            type="button"
            class="theme-pill-btn flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-bold transition"
            :class="isDarkMode ? 'theme-pill-active' : 'theme-pill-idle'"
            @click="setTheme(true)"
          >
            <svg class="h-3.5 w-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2">
              <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"></path>
            </svg>
            <span>داكن</span>
          </button>
        </div>

        <span class="muted-text font-mono">v1.0.0 (Linux x64)</span>
      </div>
    </header>

    <!-- App Body (Sidebar + Content) -->
    <div class="flex flex-1 overflow-hidden">
      <!-- Steam Left Sidebar (Navigation) -->
      <aside class="steam-sidebar flex w-64 flex-col justify-between shrink-0 p-3 z-10">
        <div class="space-y-4">
          <!-- Profile Badge -->
          <div class="profile-badge flex items-center gap-3 rounded-lg p-3">
            <div class="relative">
              <img :src="companyLogo" alt="Logo" class="h-9 w-9 p-1 rounded-md shadow-sm" />
              <span class="absolute bottom-0 left-0 h-2 w-2 rounded-full bg-emerald-500"></span>
            </div>
            <div>
              <div class="text-xs font-extrabold text-main">منظومة المبيعات</div>
              <div class="text-[10px] font-mono accent-blue">DOCKER // ACTIVE</div>
            </div>
          </div>

          <!-- Nav Items -->
          <nav class="space-y-1 text-xs font-bold">
            <button
              type="button"
              class="nav-link flex w-full items-center gap-3 px-3.5 py-2.5 rounded transition"
              :class="activeTab === 'setup' ? 'nav-active' : 'nav-idle'"
              @click="activeTab = 'setup'"
            >
              <svg class="h-4 w-4" fill="currentColor" viewBox="0 0 20 20">
                <path fill-rule="evenodd" d="M11.49 3.17c-.38-1.56-2.6-1.56-2.98 0a1.532 1.532 0 01-2.286.948c-1.372-.836-2.942.734-2.106 2.106.54.886.061 2.042-.947 2.287-1.561.379-1.561 2.6 0 2.978a1.532 1.532 0 01.947 2.287c-.836 1.372.734 2.942 2.106 2.106a1.532 1.532 0 012.287.947c.379 1.561 2.6 1.561 2.978 0a1.533 1.533 0 012.287-.947c1.372.836 2.942-.734 2.106-2.106a1.533 1.533 0 01.947-2.287c1.561-.379 1.561-2.6 0-2.978a1.532 1.532 0 01-.947-2.287c.836-1.372-.734-2.942-2.106-2.106a1.532 1.532 0 01-2.287-.947zM10 13a3 3 0 100-6 3 3 0 000 6z" clip-rule="evenodd"/>
              </svg>
              <span>قواعد الاستعادة والربط</span>
            </button>

            <button
              type="button"
              class="nav-link flex w-full items-center gap-3 px-3.5 py-2.5 rounded transition"
              :class="activeTab === 'logs' ? 'nav-active' : 'nav-idle'"
              @click="activeTab = 'logs'; refreshLogs()"
            >
              <svg class="h-4 w-4" fill="currentColor" viewBox="0 0 20 20">
                <path d="M9 2a1 1 0 000 2h2a1 1 0 100-2H9z"/>
                <path fill-rule="evenodd" d="M4 5a2 2 0 012-2 3 3 0 003 3h2a3 3 0 003-3 2 2 0 012 2v11a2 2 0 01-2 2H6a2 2 0 01-2-2V5zm3 4a1 1 0 000 2h.01a1 1 0 100-2H7zm3 0a1 1 0 000 2h3a1 1 0 100-2h-3zm-3 4a1 1 0 100 2h.01a1 1 0 100-2H7zm3 0a1 1 0 100 2h3a1 1 0 100-2h-3z" clip-rule="evenodd"/>
              </svg>
              <span>سجل الأحداث والعمليات</span>
            </button>
          </nav>
        </div>

        <!-- Sidebar Footer Status -->
        <div class="sidebar-status rounded-lg p-3 text-[11px] space-y-1">
          <div class="flex items-center gap-2">
            <span class="h-2 w-2 rounded-full bg-emerald-500"></span>
            <span class="font-bold text-main">محرك الحاويات: نشط</span>
          </div>
          <div class="muted-text font-mono truncate" dir="ltr">{{ configDir || '~/.config' }}</div>
        </div>
      </aside>

      <!-- Steam Main Content Area -->
      <main class="steam-main flex-1 overflow-y-auto p-8">
        <div class="mx-auto max-w-4xl space-y-6">
          <!-- Feedback Alert -->
          <div
            v-if="feedback"
            class="feedback-alert flex items-center justify-between rounded-lg px-4 py-3 text-xs font-semibold"
            :class="feedback.type === 'ok' ? 'feedback-ok' : 'feedback-err'"
          >
            <div class="flex items-center gap-2">
              <span>{{ feedback.type === 'ok' ? '✓' : '✕' }}</span>
              <span>{{ feedback.text }}</span>
            </div>
            <button class="opacity-70 hover:opacity-100" @click="feedback = null">✕</button>
          </div>

          <!-- TAB 1: SETUP -->
          <div v-if="activeTab === 'setup'" class="space-y-6">
            <!-- Header Bar -->
            <div class="flex items-center justify-between pb-4 border-b border-subtle">
              <div>
                <h2 class="text-2xl font-black text-main tracking-wide">الربط واستعادة الخدمات</h2>
                <p class="text-xs muted-text mt-1">تحديد رابط الخادم وقواعد الإنعاش التلقائي عند تعطل الخدمات</p>
              </div>

              <button
                :disabled="isSaving"
                class="steam-btn-green flex items-center gap-2 px-6 py-2.5 rounded text-xs font-black text-white uppercase tracking-wider transition disabled:opacity-50"
                @click="saveAllConfigs"
              >
                <svg v-if="!isSaving" class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M5 13l4 4L19 7"></path>
                </svg>
                <span>{{ isSaving ? 'جارٍ الحفظ...' : 'حفظ الإعدادات' }}</span>
              </button>
            </div>

            <!-- Target URL Box -->
            <div class="steam-box rounded-lg p-6 space-y-4 shadow-sm">
              <div class="flex items-center justify-between">
                <h3 class="text-sm font-extrabold uppercase tracking-wider accent-blue flex items-center gap-2">
                  <span class="h-2 w-2 rounded-full bg-sky-500"></span>
                  <span>الهدف الأساسي للمنظومة</span>
                </h3>
                <span class="text-[11px] font-mono muted-text">TARGET CONFIG</span>
              </div>

              <div class="grid grid-cols-1 md:grid-cols-4 gap-4">
                <div class="md:col-span-3 space-y-1.5">
                  <label class="block text-xs font-bold text-main">رابط المنظومة (URL)</label>
                  <input
                    v-model="targetUrl"
                    type="url"
                    dir="ltr"
                    placeholder="http://127.0.0.1:8000"
                    class="steam-input w-full rounded px-4 py-2.5 text-xs font-mono focus:outline-none transition"
                  />
                  <p class="text-[11px] muted-text">الرابط المحلي الذي يستضيف تطبيق المبيعات</p>
                </div>

                <div class="space-y-1.5">
                  <label class="block text-xs font-bold text-main">مهلة الفحص</label>
                  <div class="flex items-center gap-2">
                    <input
                      v-model.number="startupTimeoutSeconds"
                      type="number"
                      min="1"
                      max="120"
                      class="steam-input w-full rounded px-3 py-2 text-xs font-mono text-center focus:outline-none"
                    />
                    <span class="text-xs muted-text font-bold">ثوانٍ</span>
                  </div>
                </div>
              </div>
            </div>

            <!-- Recovery Rules Box -->
            <div class="steam-box rounded-lg p-6 space-y-5 shadow-sm">
              <div class="flex items-center justify-between">
                <div>
                  <h3 class="text-sm font-extrabold uppercase tracking-wider accent-blue flex items-center gap-2">
                    <span class="h-2 w-2 rounded-full bg-sky-500"></span>
                    <span>قواعد وأوامر الإنعاش التلقائي</span>
                  </h3>
                  <p class="text-[11px] muted-text mt-0.5">عند رصد الخطأ يتم تشغيل الأوامر بالترتيب</p>
                </div>
                <button
                  type="button"
                  class="steam-btn-secondary px-4 py-1.5 rounded text-xs font-bold transition flex items-center gap-1"
                  @click="addRule"
                >
                  <span>+ إضافة قاعدة</span>
                </button>
              </div>

              <!-- Rules Cards -->
              <div class="space-y-4">
                <div
                  v-for="(rule, rIdx) in rules"
                  :key="rIdx"
                  class="rule-card rounded-lg p-5 space-y-4 border border-subtle"
                >
                  <div class="flex flex-wrap items-center justify-between gap-3 pb-3 border-b border-subtle">
                    <div class="flex items-center gap-2 flex-1 min-w-[260px]">
                      <span class="rule-badge px-2 py-0.5 rounded text-[10px] font-mono font-bold">
                        RULE #{{ rIdx + 1 }}
                      </span>
                      <select
                        v-model="rule.errorKind"
                        class="steam-input steam-select rounded px-3 py-1.5 text-xs font-bold focus:outline-none flex-1"
                      >
                        <option value="connection_refused">توقف الخدمة / رفض الاتصال (Connection Refused)</option>
                        <option value="http_status">رمز استجابة ويب خاطئ (HTTP 500 / 502 / 503)</option>
                        <option value="timeout">تجاوز مهلة الاستجابة (Connection Timeout)</option>
                      </select>
                    </div>

                    <div v-if="rule.errorKind === 'http_status'" class="w-36">
                      <input
                        v-model="rule.httpCodesText"
                        dir="ltr"
                        placeholder="500, 502, 503"
                        class="steam-input w-full rounded px-2.5 py-1 text-xs font-mono text-center"
                      />
                    </div>

                    <button
                      v-if="rules.length > 1"
                      type="button"
                      class="text-xs font-bold text-rose-500 hover:underline px-2 py-1"
                      @click="removeRule(rIdx)"
                    >
                      حذف القاعدة
                    </button>
                  </div>

                  <!-- Steps Inside Rule -->
                  <div class="space-y-2">
                    <div class="flex items-center justify-between text-xs">
                      <span class="font-bold text-main">خطوات الإصلاح التسلسلية:</span>
                      <button
                        type="button"
                        class="accent-blue hover:underline text-xs font-bold"
                        @click="addStep(rule)"
                      >
                        + أمر إضافي
                      </button>
                    </div>

                    <div
                      v-for="(step, sIdx) in rule.steps"
                      :key="sIdx"
                      class="step-item flex items-center gap-3 rounded p-2.5 border border-subtle"
                    >
                      <span class="text-xs font-mono font-bold muted-text">{{ sIdx + 1 }}.</span>
                      <input
                        v-model="step.commandText"
                        dir="ltr"
                        placeholder="docker restart webapp_container"
                        class="steam-input flex-1 rounded px-3 py-1.5 text-xs font-mono cmd-text focus:outline-none"
                      />
                      <div class="flex items-center gap-1.5 text-xs muted-text">
                        <span>انتظار:</span>
                        <input
                          v-model.number="step.waitSeconds"
                          type="number"
                          min="0"
                          class="steam-input w-12 rounded px-1.5 py-1 text-center text-xs font-mono"
                        />
                        <span>ثوانٍ</span>
                      </div>
                      <button
                        v-if="rule.steps.length > 1"
                        type="button"
                        class="text-rose-500 hover:text-rose-400 p-1"
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

          <!-- TAB 2: LOGS -->
          <div v-else class="space-y-5">
            <div class="flex items-center justify-between pb-3 border-b border-subtle">
              <div>
                <h2 class="text-2xl font-black text-main tracking-wide">سجل الأحداث والعمليات</h2>
                <p class="text-xs muted-text mt-1">مراقبة محاولات الاستعادة التلقائية ومخرجات الأوامر</p>
              </div>
              <button
                type="button"
                class="steam-btn-secondary px-4 py-2 rounded text-xs font-bold transition"
                @click="refreshLogs"
              >
                تحديث السجل
              </button>
            </div>

            <div v-if="logs.length === 0" class="steam-box rounded-lg p-12 text-center text-xs muted-text">
              لم يتم تسجيل أي عمليات استعادة تلقائية بعد. النظام يعمل بشكل طبيعي.
            </div>

            <div v-else class="grid grid-cols-1 lg:grid-cols-12 gap-5">
              <!-- Logs List (5 Cols) -->
              <div class="lg:col-span-5 steam-box rounded-lg p-3 divide-y divide-subtle max-h-[500px] overflow-y-auto">
                <div
                  v-for="ev in logs"
                  :key="ev.id"
                  class="log-row p-3 rounded cursor-pointer transition text-xs space-y-1.5"
                  :class="selectedEventId === ev.id ? 'log-active' : ''"
                  @click="selectLog(ev.id)"
                >
                  <div class="flex items-center justify-between">
                    <span class="font-bold text-main">{{ ev.triggerType }}</span>
                    <span
                      class="px-2 py-0.5 rounded text-[10px] font-bold"
                      :class="ev.status === 'succeeded' ? 'badge-success' : 'badge-error'"
                    >
                      {{ ev.status }}
                    </span>
                  </div>
                  <div class="text-[11px] font-mono muted-text" dir="ltr">{{ ev.startedAt }}</div>
                </div>
              </div>

              <!-- Executions Console (7 Cols) -->
              <div class="lg:col-span-7 steam-box rounded-lg p-4 space-y-4 max-h-[500px] overflow-y-auto">
                <div class="text-xs font-bold text-main border-b border-subtle pb-2 flex justify-between">
                  <span>مخرجات الأوامر المنفذة</span>
                  <span v-if="selectedEventId" class="font-mono accent-blue">حدث #{{ selectedEventId }}</span>
                </div>

                <div v-if="executions.length === 0" class="text-xs muted-text py-8 text-center">
                  اختر حدثاً من القائمة الجانبية لعرض المخرجات.
                </div>

                <div
                  v-for="ex in executions"
                  :key="ex.id"
                  class="terminal-box rounded p-3 text-xs font-mono space-y-2 border border-subtle"
                  dir="ltr"
                >
                  <div class="terminal-cmd font-bold">$ {{ ex.executable }} {{ ex.arguments }}</div>
                  <div class="text-[11px] muted-text">
                    status: <span class="text-main">{{ ex.status }}</span> | exit: {{ ex.exitCode }} | {{ ex.durationMs }}ms
                  </div>
                  <pre v-if="ex.stdout" class="terminal-output p-2.5 rounded overflow-x-auto text-[11px]">{{ ex.stdout }}</pre>
                  <pre v-if="ex.stderr" class="terminal-error p-2.5 rounded overflow-x-auto text-[11px]">{{ ex.stderr }}</pre>
                </div>
              </div>
            </div>
          </div>
        </div>
      </main>
    </div>
  </div>
</template>

<style scoped>
/* Theme Variables & Transitions */
.steam-client {
  transition: background-color 0.25s ease, color 0.25s ease;
}

/* DARK THEME (Steam Classic Client) */
.dark-theme {
  --bg-client: #171d25;
  --bg-topbar: #171a21;
  --bg-sidebar: #12161c;
  --bg-main: #171d25;
  --bg-box: #1b2838;
  --bg-card: #212b36;
  --bg-input: #0e141b;
  --bg-terminal: #0e141b;
  --border-color: #2e3c4d;
  --text-main: #c6d4df;
  --text-title: #ffffff;
  --text-muted: #8091a2;
  --accent-blue: #66c0f4;
  --accent-green: #5c7e10;
  --cmd-color: #a4d007;
}

/* LIGHT THEME (Steam Light / Modern Clean) */
.light-theme {
  --bg-client: #f1f5f9;
  --bg-topbar: #ffffff;
  --bg-sidebar: #f8fafc;
  --bg-main: #f1f5f9;
  --bg-box: #ffffff;
  --bg-card: #f8fafc;
  --bg-input: #ffffff;
  --bg-terminal: #0f172a;
  --border-color: #e2e8f0;
  --text-main: #334155;
  --text-title: #0f172a;
  --text-muted: #64748b;
  --accent-blue: #0284c7;
  --accent-green: #16a34a;
  --cmd-color: #4ade80;
}

/* Element Styles Based On Variables */
.steam-top-bar {
  background: var(--bg-topbar);
  border-bottom: 1px solid var(--border-color);
}
.title-text { color: var(--text-title); }
.muted-text { color: var(--text-muted); }
.text-main { color: var(--text-main); }
.accent-blue { color: var(--accent-blue); }

.steam-sidebar {
  background: var(--bg-sidebar);
  border-left: 1px solid var(--border-color);
}
.profile-badge {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
}
.sidebar-status {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
}

.nav-idle {
  color: var(--text-muted);
}
.nav-idle:hover {
  background: var(--bg-card);
  color: var(--text-title);
}
.nav-active {
  background: var(--bg-card);
  color: var(--accent-blue);
  border-right: 3px solid var(--accent-blue);
}

.steam-main {
  background: var(--bg-main);
}
.steam-box {
  background: var(--bg-box);
  border: 1px solid var(--border-color);
}
.rule-card {
  background: var(--bg-card);
}
.step-item {
  background: var(--bg-input);
}
.border-subtle {
  border-color: var(--border-color);
}

.steam-input {
  background-color: var(--bg-input) !important;
  border: 1px solid var(--border-color);
  color: var(--text-main) !important;
  color-scheme: dark;
}
.dark-theme .steam-input {
  color-scheme: dark;
  background-color: #0e141b !important;
  color: #c6d4df !important;
}
.light-theme .steam-input {
  color-scheme: light;
  background-color: #ffffff !important;
  color: #334155 !important;
}
.steam-input:focus {
  border-color: var(--accent-blue);
}

/* Custom Styled Select with Dark Background and Arrow */
.steam-select {
  appearance: none;
  -webkit-appearance: none;
  -moz-appearance: none;
  background-image: url("data:image/svg+xml;charset=UTF-8,%3csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='%2366c0f4' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3e%3cpolyline points='6 9 12 15 18 9'%3e%3c/polyline%3e%3c/svg%3e");
  background-repeat: no-repeat;
  background-position: left 0.75rem center;
  background-size: 1rem;
  padding-left: 2.25rem !important;
  cursor: pointer;
}
.light-theme .steam-select {
  background-image: url("data:image/svg+xml;charset=UTF-8,%3csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='%230284c7' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3e%3cpolyline points='6 9 12 15 18 9'%3e%3c/polyline%3e%3c/svg%3e");
}

.steam-select option {
  background-color: var(--bg-card) !important;
  color: var(--text-main) !important;
  padding: 6px;
}
.cmd-text {
  color: var(--cmd-color);
}

.rule-badge {
  background: rgba(102, 192, 244, 0.15);
  color: var(--accent-blue);
  border: 1px solid rgba(102, 192, 244, 0.3);
}

.steam-btn-green {
  background: linear-gradient(90deg, #5c7e10 0%, #75a017 100%);
  box-shadow: 0 0 10px rgba(117, 160, 23, 0.25);
}
.steam-btn-green:hover {
  background: linear-gradient(90deg, #6c9313 0%, #8bc01b 100%);
}

.steam-btn-secondary {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  color: var(--text-main);
}
.steam-btn-secondary:hover {
  border-color: var(--accent-blue);
  color: var(--accent-blue);
}

.theme-switch-container {
  background: var(--bg-card);
}
.theme-pill-idle {
  color: var(--text-muted);
}
.theme-pill-idle:hover {
  color: var(--text-title);
}
.theme-pill-active {
  background: var(--bg-box);
  color: var(--accent-blue);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
}

/* Feedback Alert */
.feedback-ok {
  background: rgba(34, 197, 94, 0.15);
  color: #16a34a;
  border: 1px solid rgba(34, 197, 94, 0.3);
}
.feedback-err {
  background: rgba(239, 68, 68, 0.15);
  color: #ef4444;
  border: 1px solid rgba(239, 68, 68, 0.3);
}

/* Terminal Console */
.terminal-box {
  background: var(--bg-terminal);
}
.terminal-cmd {
  color: #4ade80;
}
.terminal-output {
  background: rgba(0, 0, 0, 0.3);
  color: #93c5fd;
  border: 1px solid rgba(255, 255, 255, 0.08);
}
.terminal-error {
  background: rgba(153, 27, 27, 0.3);
  color: #fca5a5;
  border: 1px solid rgba(239, 68, 68, 0.3);
}

/* Badges */
.badge-success {
  background: rgba(34, 197, 94, 0.15);
  color: #16a34a;
  border: 1px solid rgba(34, 197, 94, 0.3);
}
.badge-error {
  background: rgba(239, 68, 68, 0.15);
  color: #ef4444;
  border: 1px solid rgba(239, 68, 68, 0.3);
}
.log-row:hover {
  background: var(--bg-card);
}
.log-active {
  background: var(--bg-card);
  border-right: 3px solid var(--accent-blue);
}
</style>
