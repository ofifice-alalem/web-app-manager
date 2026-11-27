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
      { commandText: 'docker start perfumes_app', waitSeconds: 2 },
      { commandText: 'docker start perfumes_db', waitSeconds: 2 },
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
    steps: [{ commandText: 'docker start perfumes_app', waitSeconds: 30 }],
  });
}

function removeRule(idx: number) {
  if (rules.value.length > 1) {
    rules.value.splice(idx, 1);
  }
}

function addStep(rule: SimpleRuleForm) {
  rule.steps.push({ commandText: '', waitSeconds: 30 });
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
            const exe = act.platform_commands?.windows?.executable || act.executable || '';
            const args = act.platform_commands?.windows?.args || act.args || [];
            const nextAct = actions[i + 1];
            const waitSeconds = nextAct?.type === 'wait' ? Number(nextAct.seconds) || 30 : 30;
            steps.push({
              commandText: [exe.replace(/\.exe$/i, ''), ...args].filter(Boolean).join(' '),
              waitSeconds,
            });
          }
        }

        if (steps.length === 0) {
          steps.push({ commandText: 'docker start perfumes_app', waitSeconds: 30 });
        }

        return { errorKind, httpCodesText, steps };
      });
    }
  } catch (err) {
    feedback.value = { type: 'err', text: String(err) };
  }
}

async function saveAllConfigs() {
  feedback.value = null;
  const cleanUrl = targetUrl.value.trim();
  if (!cleanUrl.startsWith('http://') && !cleanUrl.startsWith('https://')) {
    feedback.value = { type: 'err', text: 'يرجى إدخال رابط صحيح يبدأ بـ http:// أو https://' };
    return;
  }

  isSaving.value = true;
  try {
    const appPayload = {
      application: {
        id: 'main-system',
        name: 'System Host',
        url: cleanUrl,
        startup_timeout_seconds: Math.max(1, Number(startupTimeoutSeconds.value) || 5),
        reload_after_recovery: true,
      },
    };

    const servicesPayload = {
      services: [
        {
          id: 'main-app',
          name: 'Main Application',
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
        type: 'check_service',
        service_id: 'main-app',
        timeout_seconds: 30,
      });
      actions.push({
        type: 'reload_application',
        enabled: true,
      });

      return {
        id: `auto-rule-${idx + 1}`,
        name: `Auto Recovery Rule ${idx + 1}`,
        enabled: true,
        trigger,
        actions,
        retry: { max_attempts: 3, delay_seconds: 2 },
        cooldown_seconds: 5,
      };
    });

    await invoke<string>('save_config_file', {
      filename: 'app.json',
      content: JSON.stringify(appPayload, null, 2),
    });
    await invoke<string>('save_config_file', {
      filename: 'services.json',
      content: JSON.stringify(servicesPayload, null, 2),
    });
    await invoke<string>('save_config_file', {
      filename: 'recovery-rules.json',
      content: JSON.stringify({ rules: generatedRules }, null, 2),
    });

    feedback.value = { type: 'ok', text: `تم حفظ الإعدادات بنجاح في ${configDir.value}` };
  } catch (err) {
    feedback.value = { type: 'err', text: String(err) };
  } finally {
    isSaving.value = false;
  }
}

async function refreshLogs() {
  try {
    logs.value = await invoke<LogEvent[]>('load_recent_logs', { limit: 50 });
  } catch (err) {
    feedback.value = { type: 'err', text: String(err) };
  }
}

async function selectLog(id: number) {
  selectedEventId.value = id;
  try {
    executions.value = await invoke<CommandExecution[]>('load_event_executions', { eventId: id });
  } catch (err) {
    feedback.value = { type: 'err', text: String(err) };
  }
}

onMounted(async () => {
  await loadAll();
  await refreshLogs();
});
</script>

<template>
  <div dir="rtl" class="flex h-screen flex-col overflow-hidden bg-[#070B14] text-slate-100">
    <!-- Top Header Bar -->
    <header class="shrink-0 border-b border-slate-800 bg-[#0D1322] px-6 py-3.5">
      <div class="mx-auto flex max-w-5xl items-center justify-between gap-4">
        <div class="flex items-center gap-3">
          <div class="flex h-10 w-10 items-center justify-center rounded-xl border border-slate-700 bg-[#070B14] p-1.5 shadow-lg">
            <img :src="companyLogo" alt="Company Logo" class="h-full w-full object-contain" />
          </div>
          <div>
            <h1 class="text-sm font-extrabold text-white">إعدادات إصلاح المنظومة</h1>
            <p class="text-[11px] text-slate-400">الرابط ← الخطأ ← أوامر الإصلاح</p>
          </div>
        </div>

        <nav class="flex items-center gap-1 rounded-xl border border-slate-800 bg-[#070B14] p-1">
          <button
            class="rounded-lg px-4 py-1.5 text-xs font-bold transition"
            :class="activeTab === 'setup' ? 'bg-cyan-500 text-slate-950' : 'text-slate-400 hover:text-white'"
            @click="activeTab = 'setup'"
          >
            الإعدادات
          </button>
          <button
            class="rounded-lg px-4 py-1.5 text-xs font-bold transition"
            :class="activeTab === 'logs' ? 'bg-cyan-500 text-slate-950' : 'text-slate-400 hover:text-white'"
            @click="activeTab = 'logs'; refreshLogs()"
          >
            السجلات
          </button>
        </nav>

        <button
          :disabled="isSaving"
          class="rounded-xl bg-gradient-to-r from-emerald-400 to-cyan-400 px-5 py-2 text-xs font-extrabold text-slate-950 shadow-lg shadow-emerald-500/20 transition hover:brightness-110 disabled:opacity-50"
          @click="saveAllConfigs"
        >
          {{ isSaving ? 'جارٍ الحفظ...' : 'حفظ الإعدادات' }}
        </button>
      </div>
    </header>

    <!-- Main Scrollable Area -->
    <main class="flex-1 overflow-y-auto px-6 py-6">
      <div class="mx-auto max-w-4xl space-y-5">
        <div
          v-if="feedback"
          class="flex items-center gap-2.5 rounded-xl border px-4 py-3 text-sm font-medium"
          :class="feedback.type === 'ok'
            ? 'border-emerald-500/40 bg-emerald-500/10 text-emerald-300'
            : 'border-rose-500/40 bg-rose-500/10 text-rose-300'"
        >
          <span class="h-2 w-2 shrink-0 rounded-full" :class="feedback.type === 'ok' ? 'bg-emerald-400' : 'bg-rose-400'" />
          <span>{{ feedback.text }}</span>
        </div>

        <template v-if="activeTab === 'setup'">
          <!-- Step 1: Target URL & Connection Timeout -->
          <section class="rounded-2xl border border-slate-800 bg-[#0F172A] p-6 space-y-4 shadow-xl">
            <div class="flex items-center gap-2.5">
              <span class="flex h-7 w-7 items-center justify-center rounded-lg bg-cyan-500/20 text-xs font-extrabold text-cyan-300">1</span>
              <h2 class="text-base font-bold text-white">رابط المنظومة ومهلة الاتصال</h2>
            </div>
            <div class="grid grid-cols-1 gap-4 sm:grid-cols-4">
              <div class="sm:col-span-3">
                <label class="block text-xs font-semibold text-slate-400 mb-1.5">رابط المنظومة الأساسي (URL)</label>
                <input
                  v-model="targetUrl"
                  dir="ltr"
                  type="url"
                  placeholder="http://127.0.0.1:8000"
                  class="w-full rounded-xl border border-slate-700 bg-[#070B14] px-4 py-3 text-sm font-mono text-cyan-300 focus:border-cyan-500 focus:outline-none"
                />
              </div>
              <div>
                <label class="block text-xs font-semibold text-slate-400 mb-1.5">مهلة الاتصال (بالثواني)</label>
                <input
                  v-model.number="startupTimeoutSeconds"
                  type="number"
                  min="1"
                  max="300"
                  class="w-full rounded-xl border border-slate-700 bg-[#070B14] px-4 py-3 text-sm text-center text-white focus:border-cyan-500 focus:outline-none"
                />
              </div>
            </div>
          </section>

          <!-- Step 2 & 3: Errors -> Ordered Recovery Commands -->
          <section class="space-y-4">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-2.5">
                <span class="flex h-7 w-7 items-center justify-center rounded-lg bg-cyan-500/20 text-xs font-extrabold text-cyan-300">2</span>
                <h2 class="text-base font-bold text-white">عند حدوث خطأ على هذا الرابط، نفّذ الأوامر التالية:</h2>
              </div>
              <button
                class="rounded-xl border border-cyan-500/40 bg-cyan-500/15 px-3.5 py-2 text-xs font-bold text-cyan-300 hover:bg-cyan-500/25"
                @click="addRule"
              >
                + إضافة حالة خطأ أخرى
              </button>
            </div>

            <div
              v-for="(rule, rIdx) in rules"
              :key="rIdx"
              class="rounded-2xl border border-slate-800 bg-[#0F172A] p-6 space-y-5 shadow-xl"
            >
              <div class="flex flex-wrap items-end justify-between gap-4 border-b border-slate-800 pb-4">
                <div class="flex flex-wrap items-end gap-4 flex-1">
                  <div class="min-w-[220px] flex-1">
                    <label class="block text-xs font-semibold text-slate-400 mb-1.5">نوع الخطأ الناتج عن الرابط</label>
                    <select
                      v-model="rule.errorKind"
                      class="w-full rounded-xl border border-slate-700 bg-[#070B14] px-3.5 py-2.5 text-sm text-white focus:border-cyan-500 focus:outline-none"
                    >
                      <option value="connection_refused">توقف الخدمة / رفض الاتصال (Connection Refused)</option>
                      <option value="http_status">خطأ استجابة ويب (HTTP 500 / 503)</option>
                      <option value="timeout">انتهاء مهلة الاتصال (Timeout)</option>
                    </select>
                  </div>

                  <div v-if="rule.errorKind === 'http_status'" class="w-40">
                    <label class="block text-xs font-semibold text-slate-400 mb-1.5">كود الخطأ</label>
                    <input
                      v-model="rule.httpCodesText"
                      dir="ltr"
                      placeholder="500, 503"
                      class="w-full rounded-xl border border-slate-700 bg-[#070B14] px-3.5 py-2.5 text-sm font-mono text-amber-300 focus:border-cyan-500 focus:outline-none"
                    />
                  </div>
                </div>

                <button
                  v-if="rules.length > 1"
                  class="rounded-lg border border-rose-500/30 bg-rose-500/10 px-3 py-2 text-xs font-semibold text-rose-300 hover:bg-rose-500/20"
                  @click="removeRule(rIdx)"
                >
                  حذف الحالة
                </button>
              </div>

              <!-- Ordered Commands -->
              <div class="space-y-3">
                <div class="flex items-center justify-between">
                  <span class="text-xs font-bold text-slate-300">أوامر التنفيذ بالترتيب</span>
                  <button
                    class="rounded-lg border border-slate-700 bg-slate-800 px-3 py-1.5 text-xs font-semibold text-cyan-300 hover:bg-slate-700"
                    @click="addStep(rule)"
                  >
                    + إضافة أمر تنفيذ
                  </button>
                </div>

                <div
                  v-for="(step, sIdx) in rule.steps"
                  :key="sIdx"
                  class="flex flex-wrap items-center gap-3 rounded-xl border border-slate-800 bg-[#070B14] p-3.5"
                >
                  <span class="flex h-6 w-6 shrink-0 items-center justify-center rounded-lg bg-emerald-500/20 text-xs font-bold text-emerald-300">
                    {{ sIdx + 1 }}
                  </span>

                  <div class="flex-1 min-w-[240px]">
                    <input
                      v-model="step.commandText"
                      dir="ltr"
                      placeholder="docker start perfumes_app"
                      class="w-full rounded-lg border border-slate-700 bg-[#0F172A] px-3 py-2 text-xs font-mono text-cyan-300 focus:border-cyan-500 focus:outline-none"
                    />
                  </div>

                  <div class="flex items-center gap-2 text-xs text-slate-300">
                    <span>الانتظار بعدها:</span>
                    <input
                      v-model.number="step.waitSeconds"
                      type="number"
                      min="0"
                      class="w-20 rounded-lg border border-slate-700 bg-[#0F172A] px-2.5 py-2 text-center text-xs text-white"
                    />
                    <span>ثانية</span>
                  </div>

                  <button
                    v-if="rule.steps.length > 1"
                    class="text-xs font-semibold text-rose-400 hover:text-rose-300"
                    @click="removeStep(rule, sIdx)"
                  >
                    حذف
                  </button>
                </div>
              </div>
            </div>
          </section>
        </template>

        <!-- Secondary Logs Tab -->
        <section v-else class="space-y-4">
          <div class="flex items-center justify-between">
            <h2 class="text-base font-bold text-white">سجل عمليات الإصلاح</h2>
            <button
              class="rounded-xl border border-slate-700 bg-slate-800 px-4 py-2 text-xs font-semibold text-cyan-300 hover:bg-slate-700"
              @click="refreshLogs"
            >
              تحديث
            </button>
          </div>

          <div v-if="logs.length === 0" class="rounded-2xl border border-slate-800 bg-[#0F172A] p-8 text-center text-sm text-slate-400">
            لا توجد سجلات مسجلة بعد.
          </div>

          <div v-else class="grid grid-cols-1 gap-4 lg:grid-cols-2">
            <div class="space-y-2">
              <div
                v-for="ev in logs"
                :key="ev.id"
                class="cursor-pointer rounded-xl border p-3.5 transition"
                :class="selectedEventId === ev.id ? 'border-cyan-400 bg-[#16233B]' : 'border-slate-800 bg-[#0F172A] hover:border-slate-700'"
                @click="selectLog(ev.id)"
              >
                <div class="flex items-center justify-between text-xs">
                  <span class="font-bold text-white">{{ ev.triggerType }}</span>
                  <span
                    class="rounded-md px-2 py-0.5 font-mono text-[10px] font-bold uppercase"
                    :class="ev.status === 'succeeded' ? 'bg-emerald-500/15 text-emerald-300' : 'bg-rose-500/15 text-rose-300'"
                  >
                    {{ ev.status }}
                  </span>
                </div>
                <p class="mt-1 text-xs text-slate-400 font-mono" dir="ltr">{{ ev.startedAt }}</p>
              </div>
            </div>

            <div class="rounded-2xl border border-slate-800 bg-[#0F172A] p-4">
              <h3 class="text-sm font-bold text-white">تفاصيل الأوامر المنفذة</h3>
              <div v-if="executions.length === 0" class="mt-3 text-xs text-slate-400">
                اختر حدثاً لعرض الأوامر.
              </div>
              <div v-for="ex in executions" :key="ex.id" class="mt-3 rounded-xl border border-slate-800 bg-[#070B14] p-3 text-xs" dir="ltr">
                <div class="font-mono font-semibold text-cyan-300">{{ ex.executable }} {{ ex.arguments }}</div>
                <div class="mt-1 text-slate-400">status: {{ ex.status }} | exit: {{ ex.exitCode }} | {{ ex.durationMs }}ms</div>
                <pre v-if="ex.stdout" class="mt-2 overflow-auto rounded bg-[#0F172A] p-2 text-[11px] text-slate-200">{{ ex.stdout }}</pre>
                <pre v-if="ex.stderr" class="mt-2 overflow-auto rounded bg-rose-950/40 p-2 text-[11px] text-rose-300">{{ ex.stderr }}</pre>
              </div>
            </div>
          </div>
        </section>
      </div>
    </main>
  </div>
</template>
