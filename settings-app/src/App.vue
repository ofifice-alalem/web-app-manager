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
      { commandText: 'docker start perfumes_db perfumes_app', waitSeconds: 2 },
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
    httpCodesText: '500',
    steps: [{ commandText: 'docker start perfumes_db perfumes_app', waitSeconds: 10 }],
  });
}

function removeRule(idx: number) {
  if (rules.value.length > 1) {
    rules.value.splice(idx, 1);
  }
}

function addStep(rule: SimpleRuleForm) {
  rule.steps.push({ commandText: '', waitSeconds: 5 });
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
            const waitSeconds = nextAct?.type === 'wait' ? Number(nextAct.seconds) || 5 : 5;
            steps.push({
              commandText: [exe.replace(/\.exe$/i, ''), ...args].filter(Boolean).join(' '),
              waitSeconds,
            });
          }
        }

        if (steps.length === 0) {
          steps.push({ commandText: 'docker start perfumes_db perfumes_app', waitSeconds: 5 });
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
      text: 'تم حفظ كافة الإعدادات بنجاح في نظام التشغيل.',
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
  <div dir="rtl" class="gnome-window flex h-screen flex-col overflow-hidden text-[#e1e1e1] font-sans select-none">
    <!-- GNOME / Cinnamon HeaderBar -->
    <header class="gnome-headerbar shrink-0 flex items-center justify-between px-4 py-2.5 border-b border-[#2d2d2d] bg-[#242424] shadow-sm">
      <div class="flex items-center gap-3">
        <div class="app-icon flex h-8 w-8 items-center justify-center rounded-lg bg-[#2e2e2e] border border-[#3a3a3a] shadow-inner p-1">
          <img :src="companyLogo" alt="Logo" class="h-6 w-6 object-contain" />
        </div>
        <div>
          <h1 class="text-sm font-bold text-white tracking-wide">إعدادات مضيف التطبيق</h1>
          <p class="text-[11px] text-[#9a9996]">إدارة الاتصال والاستعادة التلقائية</p>
        </div>
      </div>

      <!-- Segmented Buttons (GtkLinked Buttons) -->
      <div class="gtk-linked flex rounded-md bg-[#1e1e1e] border border-[#353535] p-0.5 shadow-inner">
        <button
          class="gtk-tab-btn px-4 py-1.5 text-xs font-medium rounded transition flex items-center gap-1.5"
          :class="activeTab === 'setup' ? 'bg-[#3584e4] text-white shadow' : 'text-[#c0bfbc] hover:text-white hover:bg-[#2c2c2c]'"
          @click="activeTab = 'setup'"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"></path>
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"></path>
          </svg>
          التهيئة العامة
        </button>
        <button
          class="gtk-tab-btn px-4 py-1.5 text-xs font-medium rounded transition flex items-center gap-1.5"
          :class="activeTab === 'logs' ? 'bg-[#3584e4] text-white shadow' : 'text-[#c0bfbc] hover:text-white hover:bg-[#2c2c2c]'"
          @click="activeTab = 'logs'; refreshLogs()"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"></path>
          </svg>
          سجل الإصلاح
        </button>
      </div>

      <!-- Action Button -->
      <div>
        <button
          :disabled="isSaving"
          class="gtk-suggested-action px-4 py-1.5 text-xs font-bold rounded-md bg-[#26a269] hover:bg-[#2ec27e] active:bg-[#228e5c] text-white border border-[#228e5c] shadow transition disabled:opacity-50 flex items-center gap-1.5"
          @click="saveAllConfigs"
        >
          <svg v-if="!isSaving" class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path>
          </svg>
          {{ isSaving ? 'جارٍ الحفظ...' : 'حفظ التغييرات' }}
        </button>
      </div>
    </header>

    <!-- Main Content Area (GNOME Preferences Page Style) -->
    <main class="flex-1 overflow-y-auto px-6 py-5 bg-[#1e1e1e]">
      <div class="max-w-3xl mx-auto space-y-6">
        
        <!-- Feedback Banner -->
        <div
          v-if="feedback"
          class="gtk-infobar flex items-center justify-between rounded-lg px-4 py-2.5 text-xs font-medium border"
          :class="feedback.type === 'ok'
            ? 'border-[#26a269]/40 bg-[#26a269]/15 text-[#8ff0a4]'
            : 'border-[#c01c28]/40 bg-[#c01c28]/15 text-[#ff7b63]'"
        >
          <div class="flex items-center gap-2">
            <span class="inline-block w-2 h-2 rounded-full" :class="feedback.type === 'ok' ? 'bg-[#26a269]' : 'bg-[#c01c28]'"></span>
            <span>{{ feedback.text }}</span>
          </div>
          <button class="text-xs opacity-70 hover:opacity-100" @click="feedback = null">✕</button>
        </div>

        <template v-if="activeTab === 'setup'">
          <!-- Group 1: Application Connection -->
          <div class="gtk-group space-y-2">
            <div class="px-1 text-[11px] font-bold uppercase tracking-wider text-[#9a9996]">
              خادم التطبيق والمهلة
            </div>
            <div class="gtk-boxed-list rounded-xl bg-[#2d2d2d] border border-[#3b3b3b] shadow-sm divide-y divide-[#383838] overflow-hidden">
              <!-- Row 1: Target URL -->
              <div class="p-3.5 flex flex-col sm:flex-row sm:items-center justify-between gap-3 hover:bg-[#323232] transition">
                <div class="space-y-0.5">
                  <div class="text-xs font-medium text-white">رابط المنظومة (URL)</div>
                  <div class="text-[11px] text-[#9a9996]">عنوان الويب المستضاف محلياً أو على الشبكة</div>
                </div>
                <div class="sm:w-80">
                  <input
                    v-model="targetUrl"
                    dir="ltr"
                    type="url"
                    placeholder="http://127.0.0.1:8000"
                    class="gtk-entry w-full rounded-md border border-[#444444] bg-[#242424] px-3 py-1.5 text-xs font-mono text-[#78aeed] focus:border-[#3584e4] focus:outline-none transition shadow-inner"
                  />
                </div>
              </div>

              <!-- Row 2: Startup Timeout -->
              <div class="p-3.5 flex items-center justify-between gap-3 hover:bg-[#323232] transition">
                <div class="space-y-0.5">
                  <div class="text-xs font-medium text-white">مهلة بدء التشغيل</div>
                  <div class="text-[11px] text-[#9a9996]">المدة القصوى لانتظار استجابة الخادم قبل محاولة الاستعادة</div>
                </div>
                <div class="flex items-center gap-2">
                  <input
                    v-model.number="startupTimeoutSeconds"
                    type="number"
                    min="1"
                    max="180"
                    class="gtk-entry w-20 rounded-md border border-[#444444] bg-[#242424] px-2.5 py-1.5 text-xs text-center text-white focus:border-[#3584e4] focus:outline-none shadow-inner"
                  />
                  <span class="text-xs text-[#9a9996]">ثوانٍ</span>
                </div>
              </div>
            </div>
          </div>

          <!-- Group 2: Recovery Policies -->
          <div class="gtk-group space-y-2">
            <div class="flex items-center justify-between px-1">
              <span class="text-[11px] font-bold uppercase tracking-wider text-[#9a9996]">
                قواعد الكشف والإصلاح التلقائي
              </span>
              <button
                class="gtk-btn text-xs font-medium text-[#78aeed] hover:text-white px-2.5 py-1 rounded bg-[#2d2d2d] hover:bg-[#383838] border border-[#3b3b3b] transition"
                @click="addRule"
              >
                + إضافة قاعدة
              </button>
            </div>

            <!-- Rules Cards -->
            <div
              v-for="(rule, rIdx) in rules"
              :key="rIdx"
              class="gtk-card rounded-xl bg-[#2d2d2d] border border-[#3b3b3b] shadow-sm p-4 space-y-4"
            >
              <!-- Trigger Condition Row -->
              <div class="flex flex-wrap items-center justify-between gap-3 border-b border-[#383838] pb-3">
                <div class="flex items-center gap-3 flex-1 min-w-[260px]">
                  <span class="h-6 w-6 rounded-full bg-[#3584e4]/20 text-[#78aeed] flex items-center justify-center text-xs font-bold border border-[#3584e4]/40">
                    {{ rIdx + 1 }}
                  </span>
                  <div class="flex-1">
                    <label class="block text-[11px] text-[#9a9996] mb-1">عند رصد الخطأ التالي:</label>
                    <select
                      v-model="rule.errorKind"
                      class="gtk-combo w-full rounded-md border border-[#444444] bg-[#242424] px-2.5 py-1.5 text-xs text-white focus:border-[#3584e4] focus:outline-none"
                    >
                      <option value="connection_refused">توقف الخدمة / رفض الاتصال (Connection Refused)</option>
                      <option value="http_status">رمز استجابة HTTP خاطئ (HTTP 500 / 503)</option>
                      <option value="timeout">تجاوز مهلة الاستجابة (Timeout)</option>
                    </select>
                  </div>
                </div>

                <div v-if="rule.errorKind === 'http_status'" class="w-32">
                  <label class="block text-[11px] text-[#9a9996] mb-1">أكواد الخطأ:</label>
                  <input
                    v-model="rule.httpCodesText"
                    dir="ltr"
                    placeholder="500, 503"
                    class="gtk-entry w-full rounded-md border border-[#444444] bg-[#242424] px-2.5 py-1.5 text-xs font-mono text-[#f6d32d] focus:border-[#3584e4] focus:outline-none"
                  />
                </div>

                <button
                  v-if="rules.length > 1"
                  class="gtk-destructive-btn text-xs font-medium text-[#ff7b63] hover:text-white px-2.5 py-1.5 rounded hover:bg-[#c01c28]/20 transition"
                  @click="removeRule(rIdx)"
                >
                  حذف القاعدة
                </button>
              </div>

              <!-- Command Steps -->
              <div class="space-y-2">
                <div class="flex items-center justify-between text-[11px] text-[#9a9996]">
                  <span>أوامر الاستعادة التسلسلية (تُنفذ تباعاً):</span>
                  <button
                    class="text-[#78aeed] hover:underline"
                    @click="addStep(rule)"
                  >
                    + إضافة أمر
                  </button>
                </div>

                <div
                  v-for="(step, sIdx) in rule.steps"
                  :key="sIdx"
                  class="flex flex-wrap items-center gap-2 rounded-lg bg-[#242424] p-2.5 border border-[#383838]"
                >
                  <span class="text-[10px] font-mono text-[#9a9996] px-1.5 py-0.5 rounded bg-[#2e2e2e]">
                    #{{ sIdx + 1 }}
                  </span>

                  <div class="flex-1 min-w-[200px]">
                    <input
                      v-model="step.commandText"
                      dir="ltr"
                      placeholder="docker start perfumes_db perfumes_app"
                      class="gtk-entry w-full rounded border border-[#404040] bg-[#1a1a1a] px-2.5 py-1 text-xs font-mono text-[#8ff0a4] focus:border-[#3584e4] focus:outline-none"
                    />
                  </div>

                  <div class="flex items-center gap-1.5 text-xs text-[#9a9996]">
                    <span class="text-[11px]">انتظار:</span>
                    <input
                      v-model.number="step.waitSeconds"
                      type="number"
                      min="0"
                      class="gtk-entry w-14 rounded border border-[#404040] bg-[#1a1a1a] px-2 py-1 text-center text-xs text-white"
                    />
                    <span class="text-[11px]">ث</span>
                  </div>

                  <button
                    v-if="rule.steps.length > 1"
                    class="text-[#ff7b63] hover:text-white text-xs px-1.5 py-0.5 rounded hover:bg-[#c01c28]/20"
                    @click="removeStep(rule, sIdx)"
                  >
                    ✕
                  </button>
                </div>
              </div>
            </div>
          </div>
        </template>

        <!-- Tab 2: Logs Inspector -->
        <template v-else>
          <div class="gtk-group space-y-3">
            <div class="flex items-center justify-between px-1">
              <span class="text-[11px] font-bold uppercase tracking-wider text-[#9a9996]">
                سجل محاولات التعافي الأخيرة
              </span>
              <button
                class="gtk-btn text-xs font-medium text-[#78aeed] hover:text-white px-3 py-1 rounded bg-[#2d2d2d] hover:bg-[#383838] border border-[#3b3b3b] transition"
                @click="refreshLogs"
              >
                تحديث السجل
              </button>
            </div>

            <div v-if="logs.length === 0" class="gtk-card rounded-xl bg-[#2d2d2d] border border-[#3b3b3b] p-8 text-center text-xs text-[#9a9996]">
              لا توجد أي أحداث مسجلة حتى الآن.
            </div>

            <div v-else class="grid grid-cols-1 md:grid-cols-2 gap-4">
              <!-- Events List -->
              <div class="rounded-xl bg-[#2d2d2d] border border-[#3b3b3b] divide-y divide-[#383838] max-h-[460px] overflow-y-auto">
                <div
                  v-for="ev in logs"
                  :key="ev.id"
                  class="p-3 cursor-pointer hover:bg-[#353535] transition text-xs space-y-1"
                  :class="selectedEventId === ev.id ? 'bg-[#3584e4]/15 border-r-2 border-[#3584e4]' : ''"
                  @click="selectLog(ev.id)"
                >
                  <div class="flex items-center justify-between">
                    <span class="font-bold text-white">{{ ev.triggerType }}</span>
                    <span
                      class="px-1.5 py-0.5 rounded text-[10px] font-mono font-bold"
                      :class="ev.status === 'succeeded' ? 'bg-[#26a269]/20 text-[#8ff0a4]' : 'bg-[#c01c28]/20 text-[#ff7b63]'"
                    >
                      {{ ev.status }}
                    </span>
                  </div>
                  <div class="text-[11px] font-mono text-[#9a9996]" dir="ltr">{{ ev.startedAt }}</div>
                </div>
              </div>

              <!-- Executions Details -->
              <div class="rounded-xl bg-[#242424] border border-[#3b3b3b] p-3.5 space-y-3 max-h-[460px] overflow-y-auto">
                <div class="text-xs font-bold text-white border-b border-[#383838] pb-2">
                  تفاصيل الأوامر المنفذة
                </div>
                <div v-if="executions.length === 0" class="text-xs text-[#9a9996]">
                  اختر حدثاً من القائمة الجانبية لعرض الأوامر المنفذة.
                </div>
                <div
                  v-for="ex in executions"
                  :key="ex.id"
                  class="rounded-lg bg-[#1e1e1e] border border-[#353535] p-2.5 text-xs space-y-1.5 font-mono"
                  dir="ltr"
                >
                  <div class="text-[#78aeed] font-semibold">{{ ex.executable }} {{ ex.arguments }}</div>
                  <div class="text-[11px] text-[#9a9996]">
                    status: {{ ex.status }} | exit: {{ ex.exitCode }} | {{ ex.durationMs }}ms
                  </div>
                  <pre v-if="ex.stdout" class="bg-[#121212] p-2 rounded text-[11px] text-[#e1e1e1] overflow-x-auto">{{ ex.stdout }}</pre>
                  <pre v-if="ex.stderr" class="bg-[#2a1215] p-2 rounded text-[11px] text-[#ff7b63] overflow-x-auto">{{ ex.stderr }}</pre>
                </div>
              </div>
            </div>
          </div>
        </template>

      </div>
    </main>

    <!-- GNOME Statusbar / Footer -->
    <footer class="shrink-0 px-4 py-2 border-t border-[#2d2d2d] bg-[#242424] text-[11px] text-[#9a9996] flex items-center justify-between">
      <div>مسار ملفات التهيئة: <span class="font-mono text-[#c0bfbc]" dir="ltr">{{ configDir || '~/.config/desktop-application-host' }}</span></div>
      <div>نظام التشغيل: Linux Mint / Cinnamon</div>
    </footer>
  </div>
</template>

<style scoped>
/* Adwaita / Linux Desktop Native Visual Styles */
.gnome-window {
  font-family: -apple-system, BlinkMacSystemFont, "Cantarell", "Ubuntu", "Segoe UI", "Tajawal", Roboto, sans-serif;
  background-color: #1e1e1e;
}
.gtk-entry:focus, .gtk-combo:focus {
  box-shadow: 0 0 0 2px rgba(53, 132, 228, 0.4);
}
</style>
