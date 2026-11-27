<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';

type Tab = 'app' | 'services' | 'rules' | 'logs';

interface AppForm {
  id: string;
  name: string;
  url: string;
  startup_timeout_seconds: number;
  reload_after_recovery: boolean;
}

interface PlatformCmd {
  executable: string;
  argsText: string;
  timeout_seconds: number;
}

interface ServiceForm {
  id: string;
  name: string;
  required: boolean;
  checkType: 'http' | 'tcp' | 'process' | 'docker';
  url: string;
  expectedStatusText: string;
  host: string;
  port: number;
  process_name: string;
  container_name: string;
  timeout_seconds: number;
  showCommands: boolean;
  hasStartCmd: boolean;
  startWin: PlatformCmd;
  startLinux: PlatformCmd;
  hasRestartCmd: boolean;
  restartWin: PlatformCmd;
  restartLinux: PlatformCmd;
}

interface ActionForm {
  type: 'command' | 'start_service' | 'restart_service' | 'wait' | 'check_service' | 'reload_application';
  executable: string;
  argsText: string;
  service_id: string;
  seconds: number;
  timeout_seconds: number;
  continue_on_failure: boolean;
  enabled: boolean;
}

interface RuleForm {
  id: string;
  name: string;
  enabled: boolean;
  triggerType: 'http_status' | 'connection_refused' | 'service_status' | 'tcp_unavailable' | 'process_not_running';
  triggerServiceId: string;
  triggerStatusCodesText: string;
  triggerStatusValue: 'stopped' | 'unhealthy' | 'timeout';
  max_attempts: number;
  delay_seconds: number;
  cooldown_seconds: number;
  actions: ActionForm[];
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

const activeTab = ref<Tab>('app');
const configDir = ref('');
const feedback = ref<{ type: 'ok' | 'err'; text: string } | null>(null);
const isSaving = ref(false);

const appForm = ref<AppForm>({
  id: 'sales-system',
  name: 'Sales System',
  url: 'http://127.0.0.1:8000',
  startup_timeout_seconds: 60,
  reload_after_recovery: true,
});

const servicesForm = ref<ServiceForm[]>([]);
const selectedServiceIdx = ref<number>(0);

const rulesForm = ref<RuleForm[]>([]);
const selectedRuleIdx = ref<number>(0);

const logs = ref<LogEvent[]>([]);
const selectedEventId = ref<number | null>(null);
const executions = ref<CommandExecution[]>([]);

const selectedService = computed(() => servicesForm.value[selectedServiceIdx.value] ?? null);
const selectedRule = computed(() => rulesForm.value[selectedRuleIdx.value] ?? null);

function parsePlatformCmd(cmdObj: any, defaultTimeout: number): PlatformCmd {
  if (!cmdObj) {
    return { executable: '', argsText: '', timeout_seconds: defaultTimeout };
  }
  return {
    executable: cmdObj.executable || '',
    argsText: Array.isArray(cmdObj.args) ? cmdObj.args.join(' ') : '',
    timeout_seconds: Number(cmdObj.timeout_seconds) || defaultTimeout,
  };
}

function serializePlatformCmd(cmd: PlatformCmd) {
  return {
    executable: cmd.executable.trim(),
    args: cmd.argsText.trim().split(/\s+/).filter(Boolean),
    timeout_seconds: Number(cmd.timeout_seconds) || 30,
  };
}

function addService() {
  const nextIndex = servicesForm.value.length + 1;
  servicesForm.value.push({
    id: `service-${nextIndex}`,
    name: `خدمة جديدة ${nextIndex}`,
    required: true,
    checkType: 'http',
    url: 'http://127.0.0.1:8000',
    expectedStatusText: '200',
    host: '127.0.0.1',
    port: 8000,
    process_name: '',
    container_name: '',
    timeout_seconds: 5,
    showCommands: false,
    hasStartCmd: false,
    startWin: { executable: 'docker.exe', argsText: 'compose up -d', timeout_seconds: 60 },
    startLinux: { executable: 'docker', argsText: 'compose up -d', timeout_seconds: 60 },
    hasRestartCmd: false,
    restartWin: { executable: 'docker.exe', argsText: 'compose restart app', timeout_seconds: 45 },
    restartLinux: { executable: 'docker', argsText: 'compose restart app', timeout_seconds: 45 },
  });
  selectedServiceIdx.value = servicesForm.value.length - 1;
}

function removeService(idx: number) {
  servicesForm.value.splice(idx, 1);
  if (selectedServiceIdx.value >= servicesForm.value.length) {
    selectedServiceIdx.value = Math.max(0, servicesForm.value.length - 1);
  }
}

function addRule() {
  const nextIndex = rulesForm.value.length + 1;
  const defaultSvc = servicesForm.value[0]?.id || 'sales-app';
  rulesForm.value.push({
    id: `rule-${nextIndex}`,
    name: `قاعدة إصلاح ${nextIndex}`,
    enabled: true,
    triggerType: 'service_status',
    triggerServiceId: defaultSvc,
    triggerStatusCodesText: '500',
    triggerStatusValue: 'stopped',
    max_attempts: 3,
    delay_seconds: 5,
    cooldown_seconds: 60,
    actions: [
      {
        type: 'restart_service',
        executable: '',
        argsText: '',
        service_id: defaultSvc,
        seconds: 3,
        timeout_seconds: 30,
        continue_on_failure: false,
        enabled: true,
      },
      {
        type: 'reload_application',
        executable: '',
        argsText: '',
        service_id: '',
        seconds: 3,
        timeout_seconds: 30,
        continue_on_failure: false,
        enabled: true,
      },
    ],
  });
  selectedRuleIdx.value = rulesForm.value.length - 1;
}

function removeRule(idx: number) {
  rulesForm.value.splice(idx, 1);
  if (selectedRuleIdx.value >= rulesForm.value.length) {
    selectedRuleIdx.value = Math.max(0, rulesForm.value.length - 1);
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
    if (parsedApp.application) {
      appForm.value = {
        id: parsedApp.application.id ?? 'sales-system',
        name: parsedApp.application.name ?? 'Sales System',
        url: parsedApp.application.url ?? 'http://127.0.0.1:8000',
        startup_timeout_seconds: Number(parsedApp.application.startup_timeout_seconds) || 60,
        reload_after_recovery: Boolean(parsedApp.application.reload_after_recovery),
      };
    }

    const parsedServices = JSON.parse(bundle.servicesJson);
    servicesForm.value = (parsedServices.services || []).map((s: any): ServiceForm => {
      const check = s.check || {};
      const startCmd = s.commands?.start;
      const restartCmd = s.commands?.restart;
      return {
        id: s.id || '',
        name: s.name || '',
        required: Boolean(s.required),
        checkType: check.type || 'http',
        url: check.url || 'http://127.0.0.1:8000',
        expectedStatusText: Array.isArray(check.expected_status) ? check.expected_status.join(', ') : '200',
        host: check.host || '127.0.0.1',
        port: Number(check.port) || 8000,
        process_name: check.process_name || '',
        container_name: check.container_name || '',
        timeout_seconds: Number(check.timeout_seconds) || 5,
        showCommands: false,
        hasStartCmd: Boolean(startCmd),
        startWin: parsePlatformCmd(startCmd?.platform_commands?.windows ?? startCmd, 60),
        startLinux: parsePlatformCmd(startCmd?.platform_commands?.linux ?? startCmd, 60),
        hasRestartCmd: Boolean(restartCmd),
        restartWin: parsePlatformCmd(restartCmd?.platform_commands?.windows ?? restartCmd, 45),
        restartLinux: parsePlatformCmd(restartCmd?.platform_commands?.linux ?? restartCmd, 45),
      };
    });

    const parsedRules = JSON.parse(bundle.rulesJson);
    rulesForm.value = (parsedRules.rules || []).map((r: any): RuleForm => {
      const tr = r.trigger || {};
      return {
        id: r.id || '',
        name: r.name || '',
        enabled: Boolean(r.enabled),
        triggerType: tr.type || 'service_status',
        triggerServiceId: tr.service_id || '',
        triggerStatusCodesText: Array.isArray(tr.status_codes) ? tr.status_codes.join(', ') : '500',
        triggerStatusValue: tr.value || 'stopped',
        max_attempts: Number(r.retry?.max_attempts) || 3,
        delay_seconds: Number(r.retry?.delay_seconds) || 5,
        cooldown_seconds: Number(r.cooldown_seconds) || 60,
        actions: (r.actions || []).map((a: any): ActionForm => ({
          type: a.type || 'wait',
          executable: a.executable || '',
          argsText: Array.isArray(a.args) ? a.args.join(' ') : '',
          service_id: a.service_id || '',
          seconds: Number(a.seconds) || 3,
          timeout_seconds: Number(a.timeout_seconds) || 30,
          continue_on_failure: Boolean(a.continue_on_failure),
          enabled: a.enabled !== undefined ? Boolean(a.enabled) : true,
        })),
      };
    });
  } catch (err) {
    feedback.value = { type: 'err', text: String(err) };
  }
}

function buildAppJsonString() {
  return JSON.stringify(
    {
      application: {
        id: appForm.value.id.trim(),
        name: appForm.value.name.trim(),
        url: appForm.value.url.trim(),
        startup_timeout_seconds: Number(appForm.value.startup_timeout_seconds),
        reload_after_recovery: appForm.value.reload_after_recovery,
      },
    },
    null,
    2,
  );
}

function buildServicesJsonString() {
  const services = servicesForm.value.map((s) => {
    let check: Record<string, unknown>;
    if (s.checkType === 'http') {
      const codes = s.expectedStatusText
        .split(',')
        .map((n) => Number(n.trim()))
        .filter((n) => !Number.isNaN(n) && n >= 100 && n <= 599);
      check = {
        type: 'http',
        url: s.url.trim(),
        expected_status: codes.length ? codes : [200],
        timeout_seconds: Number(s.timeout_seconds) || 5,
      };
    } else if (s.checkType === 'tcp') {
      check = {
        type: 'tcp',
        host: s.host.trim(),
        port: Number(s.port) || 8000,
        timeout_seconds: Number(s.timeout_seconds) || 5,
      };
    } else if (s.checkType === 'process') {
      check = {
        type: 'process',
        process_name: s.process_name.trim(),
        timeout_seconds: Number(s.timeout_seconds) || 5,
      };
    } else {
      check = {
        type: 'docker',
        container_name: s.container_name.trim(),
        timeout_seconds: Number(s.timeout_seconds) || 5,
      };
    }

    return {
      id: s.id.trim(),
      name: s.name.trim(),
      required: s.required,
      check,
      commands: {
        start: s.hasStartCmd
          ? {
              platform_commands: {
                windows: serializePlatformCmd(s.startWin),
                linux: serializePlatformCmd(s.startLinux),
              },
            }
          : null,
        stop: null,
        restart: s.hasRestartCmd
          ? {
              platform_commands: {
                windows: serializePlatformCmd(s.restartWin),
                linux: serializePlatformCmd(s.restartLinux),
              },
            }
          : null,
      },
    };
  });

  return JSON.stringify({ services }, null, 2);
}

function buildRulesJsonString() {
  const rules = rulesForm.value.map((r) => {
    let trigger: Record<string, unknown>;
    if (r.triggerType === 'http_status') {
      const codes = r.triggerStatusCodesText
        .split(',')
        .map((n) => Number(n.trim()))
        .filter((n) => !Number.isNaN(n) && n >= 100 && n <= 599);
      trigger = {
        type: 'http_status',
        service_id: r.triggerServiceId.trim(),
        status_codes: codes.length ? codes : [500],
      };
    } else if (r.triggerType === 'service_status') {
      trigger = {
        type: 'service_status',
        service_id: r.triggerServiceId.trim(),
        value: r.triggerStatusValue,
      };
    } else {
      trigger = {
        type: r.triggerType,
        service_id: r.triggerServiceId.trim(),
      };
    }

    const actions = r.actions.map((a) => {
      if (a.type === 'command') {
        return {
          type: 'command',
          executable: a.executable.trim(),
          args: a.argsText.trim().split(/\s+/).filter(Boolean),
          timeout_seconds: Number(a.timeout_seconds) || 30,
          continue_on_failure: a.continue_on_failure,
        };
      }
      if (a.type === 'start_service' || a.type === 'restart_service') {
        return {
          type: a.type,
          service_id: a.service_id.trim(),
          timeout_seconds: Number(a.timeout_seconds) || 30,
          continue_on_failure: a.continue_on_failure,
        };
      }
      if (a.type === 'wait') {
        return { type: 'wait', seconds: Number(a.seconds) || 3 };
      }
      if (a.type === 'check_service') {
        return {
          type: 'check_service',
          service_id: a.service_id.trim(),
          timeout_seconds: Number(a.timeout_seconds) || 30,
        };
      }
      return { type: 'reload_application', enabled: a.enabled };
    });

    return {
      id: r.id.trim(),
      name: r.name.trim(),
      enabled: r.enabled,
      trigger,
      actions,
      retry: {
        max_attempts: Number(r.max_attempts) || 1,
        delay_seconds: Number(r.delay_seconds) || 0,
      },
      cooldown_seconds: Number(r.cooldown_seconds) || 0,
    };
  });

  return JSON.stringify({ rules }, null, 2);
}

async function saveAllConfigs() {
  feedback.value = null;
  isSaving.value = true;
  try {
    await invoke<string>('save_config_file', { filename: 'app.json', content: buildAppJsonString() });
    await invoke<string>('save_config_file', { filename: 'services.json', content: buildServicesJsonString() });
    await invoke<string>('save_config_file', { filename: 'recovery-rules.json', content: buildRulesJsonString() });
    feedback.value = { type: 'ok', text: `تم حفظ جميع ملفات الإعدادات بنجاح في: ${configDir.value}` };
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
  <div dir="rtl" class="flex min-h-screen bg-slate-50 text-slate-900 font-sans">
    <!-- Right Sidebar (RTL) -->
    <aside class="w-64 shrink-0 border-l border-slate-200 bg-white p-5 flex flex-col justify-between shadow-xs">
      <div>
        <div class="pb-4 border-b border-slate-100">
          <h1 class="text-base font-bold text-slate-900">إعدادات المنظومة</h1>
          <p class="mt-1 text-[11px] text-slate-500 break-all" dir="ltr">{{ configDir }}</p>
        </div>

        <nav class="mt-5 space-y-1.5">
          <button
            class="w-full rounded-lg px-3 py-2.5 text-right text-sm font-medium transition"
            :class="activeTab === 'app' ? 'bg-slate-900 text-white shadow-xs' : 'text-slate-700 hover:bg-slate-100'"
            @click="activeTab = 'app'"
          >
            إعدادات التطبيق
          </button>
          <button
            class="w-full flex items-center justify-between rounded-lg px-3 py-2.5 text-right text-sm font-medium transition"
            :class="activeTab === 'services' ? 'bg-slate-900 text-white shadow-xs' : 'text-slate-700 hover:bg-slate-100'"
            @click="activeTab = 'services'"
          >
            <span>الخدمات</span>
            <span class="rounded-full px-2 py-0.5 text-xs" :class="activeTab === 'services' ? 'bg-slate-700 text-white' : 'bg-slate-100 text-slate-600'">
              {{ servicesForm.length }}
            </span>
          </button>
          <button
            class="w-full flex items-center justify-between rounded-lg px-3 py-2.5 text-right text-sm font-medium transition"
            :class="activeTab === 'rules' ? 'bg-slate-900 text-white shadow-xs' : 'text-slate-700 hover:bg-slate-100'"
            @click="activeTab = 'rules'"
          >
            <span>قواعد الإصلاح</span>
            <span class="rounded-full px-2 py-0.5 text-xs" :class="activeTab === 'rules' ? 'bg-slate-700 text-white' : 'bg-slate-100 text-slate-600'">
              {{ rulesForm.length }}
            </span>
          </button>
          <button
            class="w-full rounded-lg px-3 py-2.5 text-right text-sm font-medium transition"
            :class="activeTab === 'logs' ? 'bg-slate-900 text-white shadow-xs' : 'text-slate-700 hover:bg-slate-100'"
            @click="activeTab = 'logs'; refreshLogs()"
          >
            سجلات الإصلاح
          </button>
        </nav>
      </div>

      <div class="space-y-2 pt-4 border-t border-slate-100">
        <button
          :disabled="isSaving"
          class="w-full rounded-lg bg-emerald-600 px-3 py-2.5 text-sm font-semibold text-white shadow-xs transition hover:bg-emerald-700 disabled:opacity-50"
          @click="saveAllConfigs"
        >
          {{ isSaving ? 'جارٍ الحفظ...' : 'حفظ جميع الإعدادات' }}
        </button>
        <button
          class="w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-xs font-medium text-slate-700 hover:bg-slate-50"
          @click="loadAll"
        >
          إعادة تحميل من القرص
        </button>
      </div>
    </aside>

    <!-- Main Workspace -->
    <main class="flex-1 p-6 overflow-y-auto">
      <div
        v-if="feedback"
        class="mb-5 rounded-xl border px-4 py-3 text-sm font-medium"
        :class="feedback.type === 'ok' ? 'border-emerald-200 bg-emerald-50 text-emerald-800' : 'border-rose-200 bg-rose-50 text-rose-800'"
      >
        {{ feedback.text }}
      </div>

      <!-- TAB 1: Application -->
      <section v-if="activeTab === 'app'" class="max-w-2xl space-y-4">
        <div>
          <h2 class="text-lg font-bold text-slate-900">إعدادات التطبيق</h2>
          <p class="text-xs text-slate-500 mt-0.5">تحديد رابط المنظومة التي سيتم عرضها داخل نافذة البائع.</p>
        </div>

        <div class="rounded-xl border border-slate-200 bg-white p-6 space-y-5 shadow-xs">
          <div class="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <div>
              <label class="block text-xs font-semibold text-slate-700 mb-1.5">اسم المنظومة</label>
              <input v-model="appForm.name" class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm focus:border-slate-500 focus:outline-none" />
            </div>
            <div>
              <label class="block text-xs font-semibold text-slate-700 mb-1.5">المعرّف التقني</label>
              <input v-model="appForm.id" dir="ltr" class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm font-mono focus:border-slate-500 focus:outline-none" />
            </div>
          </div>

          <div>
            <label class="block text-xs font-semibold text-slate-700 mb-1.5">رابط المنظومة (URL)</label>
            <input v-model="appForm.url" dir="ltr" type="url" class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm font-mono focus:border-slate-500 focus:outline-none" />
          </div>

          <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 items-center pt-2">
            <div>
              <label class="block text-xs font-semibold text-slate-700 mb-1.5">مهلة الانتظار عند التشغيل (بالثواني)</label>
              <input v-model.number="appForm.startup_timeout_seconds" type="number" min="1" class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm focus:border-slate-500 focus:outline-none" />
            </div>
            <label class="flex items-center gap-2.5 pt-5 text-sm font-medium text-slate-700 cursor-pointer">
              <input v-model="appForm.reload_after_recovery" type="checkbox" class="h-4 w-4 rounded border-slate-300" />
              تحديث الشاشة تلقائياً بعد الإصلاح
            </label>
          </div>
        </div>
      </section>

      <!-- TAB 2: Services (Master-Detail) -->
      <section v-else-if="activeTab === 'services'" class="space-y-4">
        <div class="flex items-center justify-between">
          <div>
            <h2 class="text-lg font-bold text-slate-900">الخدمات المعتمدة</h2>
            <p class="text-xs text-slate-500 mt-0.5">اختر خدمة من القائمة لتعديل طريقة فحصها وأوامرها.</p>
          </div>
          <button
            class="rounded-lg bg-slate-900 px-3.5 py-2 text-sm font-medium text-white hover:bg-slate-800"
            @click="addService"
          >
            + إضافة خدمة
          </button>
        </div>

        <div class="grid grid-cols-1 gap-5 lg:grid-cols-3">
          <!-- Services List -->
          <div class="space-y-2">
            <div
              v-for="(svc, idx) in servicesForm"
              :key="idx"
              class="cursor-pointer rounded-xl border bg-white p-3.5 transition"
              :class="selectedServiceIdx === idx ? 'border-slate-900 ring-1 ring-slate-900 shadow-xs' : 'border-slate-200 hover:border-slate-300'"
              @click="selectedServiceIdx = idx"
            >
              <div class="flex items-center justify-between">
                <span class="text-sm font-bold text-slate-900">{{ svc.name || svc.id }}</span>
                <span
                  class="rounded-full px-2 py-0.5 text-[11px] font-medium"
                  :class="svc.required ? 'bg-amber-50 text-amber-700' : 'bg-slate-100 text-slate-600'"
                >
                  {{ svc.required ? 'إلزامية' : 'اختيارية' }}
                </span>
              </div>
              <div class="mt-1 flex items-center justify-between text-xs text-slate-500">
                <span dir="ltr" class="font-mono">{{ svc.id }}</span>
                <span class="uppercase">{{ svc.checkType }}</span>
              </div>
            </div>
          </div>

          <!-- Selected Service Editor -->
          <div v-if="selectedService" class="lg:col-span-2 rounded-xl border border-slate-200 bg-white p-6 space-y-5 shadow-xs">
            <div class="flex items-center justify-between border-b border-slate-100 pb-3">
              <h3 class="text-base font-bold text-slate-900">{{ selectedService.name || selectedService.id }}</h3>
              <button class="text-xs font-medium text-rose-600 hover:underline" @click="removeService(selectedServiceIdx)">
                حذف هذه الخدمة
              </button>
            </div>

            <div class="grid grid-cols-1 gap-4 sm:grid-cols-2">
              <div>
                <label class="block text-xs font-semibold text-slate-700 mb-1">اسم الخدمة</label>
                <input v-model="selectedService.name" class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm" />
              </div>
              <div>
                <label class="block text-xs font-semibold text-slate-700 mb-1">المعرّف التقني</label>
                <input v-model="selectedService.id" dir="ltr" class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm font-mono" />
              </div>
            </div>

            <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 items-center">
              <div>
                <label class="block text-xs font-semibold text-slate-700 mb-1">طريقة الفحص</label>
                <select v-model="selectedService.checkType" class="w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm">
                  <option value="http">فحص رابط ويب (HTTP)</option>
                  <option value="tcp">فحص منفذ شبكة (TCP)</option>
                  <option value="process">فحص عملية نظام (Process)</option>
                  <option value="docker">فحص حاوية Docker</option>
                </select>
              </div>
              <label class="flex items-center gap-2 pt-5 text-sm font-medium text-slate-700 cursor-pointer">
                <input v-model="selectedService.required" type="checkbox" class="h-4 w-4 rounded" />
                خدمة إلزامية لتشغيل المنظومة
              </label>
            </div>

            <div class="rounded-xl bg-slate-50 p-4 grid grid-cols-1 gap-4 sm:grid-cols-3">
              <template v-if="selectedService.checkType === 'http'">
                <div class="sm:col-span-2">
                  <label class="block text-xs font-semibold text-slate-700 mb-1">رابط الفحص</label>
                  <input v-model="selectedService.url" dir="ltr" class="w-full rounded-lg border border-slate-300 bg-white px-3 py-1.5 text-sm font-mono" />
                </div>
                <div>
                  <label class="block text-xs font-semibold text-slate-700 mb-1">أكواد النجاح</label>
                  <input v-model="selectedService.expectedStatusText" dir="ltr" placeholder="200, 302" class="w-full rounded-lg border border-slate-300 bg-white px-3 py-1.5 text-sm font-mono" />
                </div>
              </template>
              <template v-else-if="selectedService.checkType === 'tcp'">
                <div class="sm:col-span-2">
                  <label class="block text-xs font-semibold text-slate-700 mb-1">عنوان الخادم (Host)</label>
                  <input v-model="selectedService.host" dir="ltr" class="w-full rounded-lg border border-slate-300 bg-white px-3 py-1.5 text-sm font-mono" />
                </div>
                <div>
                  <label class="block text-xs font-semibold text-slate-700 mb-1">رقم المنفذ (Port)</label>
                  <input v-model.number="selectedService.port" type="number" class="w-full rounded-lg border border-slate-300 bg-white px-3 py-1.5 text-sm" />
                </div>
              </template>
              <template v-else-if="selectedService.checkType === 'process'">
                <div class="sm:col-span-2">
                  <label class="block text-xs font-semibold text-slate-700 mb-1">اسم العملية</label>
                  <input v-model="selectedService.process_name" dir="ltr" class="w-full rounded-lg border border-slate-300 bg-white px-3 py-1.5 text-sm font-mono" />
                </div>
              </template>
              <template v-else>
                <div class="sm:col-span-2">
                  <label class="block text-xs font-semibold text-slate-700 mb-1">اسم حاوية Docker</label>
                  <input v-model="selectedService.container_name" dir="ltr" class="w-full rounded-lg border border-slate-300 bg-white px-3 py-1.5 text-sm font-mono" />
                </div>
              </template>
              <div>
                <label class="block text-xs font-semibold text-slate-700 mb-1">مهلة الفحص (ثوانٍ)</label>
                <input v-model.number="selectedService.timeout_seconds" type="number" min="1" class="w-full rounded-lg border border-slate-300 bg-white px-3 py-1.5 text-sm" />
              </div>
            </div>

            <!-- Collapsible Commands -->
            <div class="border-t border-slate-100 pt-3">
              <button
                class="text-xs font-semibold text-slate-700 hover:text-slate-900"
                @click="selectedService.showCommands = !selectedService.showCommands"
              >
                {{ selectedService.showCommands ? '▾ إخفاء أوامر التشغيل وإعادة التشغيل' : '▸ إعدادات متقدمة: أوامر التشغيل وإعادة التشغيل' }}
              </button>

              <div v-if="selectedService.showCommands" class="mt-3 grid grid-cols-1 gap-4 sm:grid-cols-2">
                <div class="rounded-lg border border-slate-200 p-3 space-y-2">
                  <label class="flex items-center gap-2 text-xs font-semibold text-slate-700">
                    <input v-model="selectedService.hasStartCmd" type="checkbox" class="h-4 w-4 rounded" />
                    تفعيل أمر التشغيل (Start)
                  </label>
                  <div v-if="selectedService.hasStartCmd" class="space-y-1.5 text-xs" dir="ltr">
                    <input v-model="selectedService.startWin.executable" placeholder="Windows: docker.exe" class="w-full rounded border border-slate-300 px-2 py-1 font-mono" />
                    <input v-model="selectedService.startWin.argsText" placeholder="Windows args: compose up -d" class="w-full rounded border border-slate-300 px-2 py-1 font-mono" />
                    <input v-model="selectedService.startLinux.executable" placeholder="Linux: docker" class="w-full rounded border border-slate-300 px-2 py-1 font-mono" />
                    <input v-model="selectedService.startLinux.argsText" placeholder="Linux args: compose up -d" class="w-full rounded border border-slate-300 px-2 py-1 font-mono" />
                  </div>
                </div>

                <div class="rounded-lg border border-slate-200 p-3 space-y-2">
                  <label class="flex items-center gap-2 text-xs font-semibold text-slate-700">
                    <input v-model="selectedService.hasRestartCmd" type="checkbox" class="h-4 w-4 rounded" />
                    تفعيل أمر إعادة التشغيل (Restart)
                  </label>
                  <div v-if="selectedService.hasRestartCmd" class="space-y-1.5 text-xs" dir="ltr">
                    <input v-model="selectedService.restartWin.executable" placeholder="Windows: docker.exe" class="w-full rounded border border-slate-300 px-2 py-1 font-mono" />
                    <input v-model="selectedService.restartWin.argsText" placeholder="Windows args: compose restart app" class="w-full rounded border border-slate-300 px-2 py-1 font-mono" />
                    <input v-model="selectedService.restartLinux.executable" placeholder="Linux: docker" class="w-full rounded border border-slate-300 px-2 py-1 font-mono" />
                    <input v-model="selectedService.restartLinux.argsText" placeholder="Linux args: compose restart app" class="w-full rounded border border-slate-300 px-2 py-1 font-mono" />
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </section>

      <!-- TAB 3: Recovery Rules (Master-Detail) -->
      <section v-else-if="activeTab === 'rules'" class="space-y-4">
        <div class="flex items-center justify-between">
          <div>
            <h2 class="text-lg font-bold text-slate-900">قواعد الإصلاح التلقائي</h2>
            <p class="text-xs text-slate-500 mt-0.5">حدد ماذا يفعل التطبيق في الخلفية عند تعطل إحدى الخدمات.</p>
          </div>
          <button
            class="rounded-lg bg-slate-900 px-3.5 py-2 text-sm font-medium text-white hover:bg-slate-800"
            @click="addRule"
          >
            + إضافة قاعدة
          </button>
        </div>

        <div class="grid grid-cols-1 gap-5 lg:grid-cols-3">
          <!-- Rules List -->
          <div class="space-y-2">
            <div
              v-for="(rule, rIdx) in rulesForm"
              :key="rIdx"
              class="cursor-pointer rounded-xl border bg-white p-3.5 transition"
              :class="selectedRuleIdx === rIdx ? 'border-slate-900 ring-1 ring-slate-900 shadow-xs' : 'border-slate-200 hover:border-slate-300'"
              @click="selectedRuleIdx = rIdx"
            >
              <div class="flex items-center justify-between">
                <span class="text-sm font-bold text-slate-900">{{ rule.name || rule.id }}</span>
                <span
                  class="rounded-full px-2 py-0.5 text-[11px] font-medium"
                  :class="rule.enabled ? 'bg-emerald-50 text-emerald-700' : 'bg-slate-100 text-slate-500'"
                >
                  {{ rule.enabled ? 'مفعّلة' : 'معطّلة' }}
                </span>
              </div>
              <p class="mt-1 text-xs text-slate-500" dir="ltr">{{ rule.triggerServiceId }} • {{ rule.triggerType }}</p>
            </div>
          </div>

          <!-- Selected Rule Editor -->
          <div v-if="selectedRule" class="lg:col-span-2 rounded-xl border border-slate-200 bg-white p-6 space-y-5 shadow-xs">
            <div class="flex items-center justify-between border-b border-slate-100 pb-3">
              <label class="flex items-center gap-2 text-base font-bold text-slate-900 cursor-pointer">
                <input v-model="selectedRule.enabled" type="checkbox" class="h-4 w-4 rounded" />
                {{ selectedRule.name || selectedRule.id }}
              </label>
              <button class="text-xs font-medium text-rose-600 hover:underline" @click="removeRule(selectedRuleIdx)">
                حذف القاعدة
              </button>
            </div>

            <div class="grid grid-cols-1 gap-4 sm:grid-cols-2">
              <div>
                <label class="block text-xs font-semibold text-slate-700 mb-1">اسم القاعدة</label>
                <input v-model="selectedRule.name" class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm" />
              </div>
              <div>
                <label class="block text-xs font-semibold text-slate-700 mb-1">المعرّف التقني</label>
                <input v-model="selectedRule.id" dir="ltr" class="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm font-mono" />
              </div>
            </div>

            <!-- Trigger -->
            <div class="rounded-xl bg-slate-50 p-4 space-y-3">
              <div class="text-xs font-bold text-slate-800">متى تعمل هذه القاعدة؟</div>
              <div class="grid grid-cols-1 gap-3 sm:grid-cols-3">
                <div>
                  <label class="block text-xs text-slate-600 mb-1">الخدمة المستهدفة</label>
                  <select v-model="selectedRule.triggerServiceId" class="w-full rounded-lg border border-slate-300 bg-white px-2.5 py-2 text-sm">
                    <option v-for="svc in servicesForm" :key="svc.id" :value="svc.id">
                      {{ svc.name }} ({{ svc.id }})
                    </option>
                  </select>
                </div>
                <div>
                  <label class="block text-xs text-slate-600 mb-1">نوع العطل</label>
                  <select v-model="selectedRule.triggerType" class="w-full rounded-lg border border-slate-300 bg-white px-2.5 py-2 text-sm">
                    <option value="service_status">تغيّر حالة الخدمة</option>
                    <option value="http_status">خطأ استجابة ويب (HTTP)</option>
                    <option value="connection_refused">رفض الاتصال بالخدمة</option>
                    <option value="tcp_unavailable">تعذر الاتصال بالمنفذ (TCP)</option>
                    <option value="process_not_running">توقف عملية النظام</option>
                  </select>
                </div>
                <div v-if="selectedRule.triggerType === 'service_status'">
                  <label class="block text-xs text-slate-600 mb-1">الحالة المكتشفة</label>
                  <select v-model="selectedRule.triggerStatusValue" class="w-full rounded-lg border border-slate-300 bg-white px-2.5 py-2 text-sm">
                    <option value="stopped">متوقفة (stopped)</option>
                    <option value="unhealthy">غير سليمة (unhealthy)</option>
                    <option value="timeout">انتهت المهلة (timeout)</option>
                  </select>
                </div>
                <div v-else-if="selectedRule.triggerType === 'http_status'">
                  <label class="block text-xs text-slate-600 mb-1">أكواد الخطأ (مثل 500)</label>
                  <input v-model="selectedRule.triggerStatusCodesText" dir="ltr" class="w-full rounded-lg border border-slate-300 bg-white px-2.5 py-2 text-sm font-mono" />
                </div>
              </div>
            </div>

            <!-- Retry & Cooldown -->
            <div class="grid grid-cols-3 gap-3">
              <div>
                <label class="block text-xs font-semibold text-slate-700 mb-1">أقصى عدد للمحاولات</label>
                <input v-model.number="selectedRule.max_attempts" type="number" min="1" class="w-full rounded-lg border border-slate-300 px-3 py-1.5 text-sm" />
              </div>
              <div>
                <label class="block text-xs font-semibold text-slate-700 mb-1">الانتظار بين المحاولات (ث)</label>
                <input v-model.number="selectedRule.delay_seconds" type="number" min="0" class="w-full rounded-lg border border-slate-300 px-3 py-1.5 text-sm" />
              </div>
              <div>
                <label class="block text-xs font-semibold text-slate-700 mb-1">فترة التهدئة (ث)</label>
                <input v-model.number="selectedRule.cooldown_seconds" type="number" min="0" class="w-full rounded-lg border border-slate-300 px-3 py-1.5 text-sm" />
              </div>
            </div>

            <!-- Ordered Actions -->
            <div class="space-y-2.5">
              <div class="flex items-center justify-between">
                <span class="text-xs font-bold text-slate-800">خطوات الإصلاح بالترتيب</span>
                <button
                  class="rounded-lg border border-slate-300 px-2.5 py-1 text-xs font-medium hover:bg-slate-50"
                  @click="selectedRule.actions.push({ type: 'wait', executable: '', argsText: '', service_id: selectedRule.triggerServiceId, seconds: 3, timeout_seconds: 30, continue_on_failure: false, enabled: true })"
                >
                  + إضافة خطوة
                </button>
              </div>
              <div
                v-for="(act, aIdx) in selectedRule.actions"
                :key="aIdx"
                class="flex flex-wrap items-center gap-2 rounded-lg border border-slate-200 bg-slate-50 p-2.5 text-xs"
              >
                <span class="flex h-5 w-5 items-center justify-center rounded-full bg-slate-900 text-[11px] font-bold text-white">
                  {{ aIdx + 1 }}
                </span>
                <select v-model="act.type" class="rounded-lg border border-slate-300 bg-white px-2 py-1.5">
                  <option value="restart_service">إعادة تشغيل الخدمة</option>
                  <option value="start_service">تشغيل الخدمة</option>
                  <option value="wait">انتظار</option>
                  <option value="check_service">فحص الخدمة</option>
                  <option value="reload_application">إعادة تحميل الشاشة</option>
                  <option value="command">تنفيذ أمر</option>
                </select>

                <template v-if="act.type === 'wait'">
                  <input v-model.number="act.seconds" type="number" min="1" class="w-20 rounded-lg border border-slate-300 bg-white px-2 py-1" />
                  <span>ثانية</span>
                </template>
                <template v-else-if="act.type === 'reload_application'">
                  <label class="flex items-center gap-1.5"><input v-model="act.enabled" type="checkbox" /> مفعّل</label>
                </template>
                <template v-else-if="act.type === 'command'">
                  <input v-model="act.executable" dir="ltr" placeholder="البرنامج" class="w-32 rounded-lg border border-slate-300 bg-white px-2 py-1 font-mono" />
                  <input v-model="act.argsText" dir="ltr" placeholder="المعاملات" class="flex-1 rounded-lg border border-slate-300 bg-white px-2 py-1 font-mono" />
                </template>
                <template v-else>
                  <select v-model="act.service_id" class="rounded-lg border border-slate-300 bg-white px-2 py-1">
                    <option v-for="svc in servicesForm" :key="svc.id" :value="svc.id">{{ svc.name }}</option>
                  </select>
                  <input v-model.number="act.timeout_seconds" type="number" min="1" class="w-20 rounded-lg border border-slate-300 bg-white px-2 py-1" />
                  <span>ث مهلة</span>
                </template>

                <button class="mr-auto text-rose-600 hover:underline" @click="selectedRule.actions.splice(aIdx, 1)">حذف</button>
              </div>
            </div>
          </div>
        </div>
      </section>

      <!-- TAB 4: Logs -->
      <section v-else class="space-y-4">
        <div class="flex items-center justify-between">
          <div>
            <h2 class="text-lg font-bold text-slate-900">سجلات الإصلاح المحلية</h2>
            <p class="text-xs text-slate-500 mt-0.5">آخر 50 حدثاً مسجلاً في قاعدة بيانات تطبيق البائع (SQLite).</p>
          </div>
          <button
            class="rounded-lg border border-slate-300 bg-white px-3.5 py-2 text-xs font-medium hover:bg-slate-50"
            @click="refreshLogs"
          >
            تحديث السجلات
          </button>
        </div>

        <div v-if="logs.length === 0" class="rounded-xl border border-slate-200 bg-white p-6 text-sm text-slate-500">
          لا توجد سجلات مسجلة بعد.
        </div>

        <div v-else class="grid grid-cols-1 gap-4 lg:grid-cols-2">
          <div class="space-y-2">
            <div
              v-for="ev in logs"
              :key="ev.id"
              class="cursor-pointer rounded-xl border bg-white p-3.5 transition hover:border-slate-400"
              :class="selectedEventId === ev.id ? 'border-slate-900 ring-1 ring-slate-900' : 'border-slate-200'"
              @click="selectLog(ev.id)"
            >
              <div class="flex items-center justify-between text-xs">
                <span class="font-bold">{{ ev.serviceId || ev.eventType }}</span>
                <span class="rounded bg-slate-100 px-2 py-0.5 font-mono">{{ ev.status }}</span>
              </div>
              <p class="mt-1 text-xs text-slate-500" dir="ltr">{{ ev.triggerType }} — {{ ev.startedAt }}</p>
            </div>
          </div>

          <div class="rounded-xl border border-slate-200 bg-white p-4">
            <h3 class="text-sm font-bold">تفاصيل الأوامر المنفذة</h3>
            <div v-if="executions.length === 0" class="mt-3 text-xs text-slate-500">
              اختر حدثاً من القائمة لعرض تفاصيل الأوامر.
            </div>
            <div v-for="ex in executions" :key="ex.id" class="mt-3 rounded-lg bg-slate-50 p-3 text-xs" dir="ltr">
              <div class="font-mono font-semibold">{{ ex.executable }} {{ ex.arguments }}</div>
              <div class="mt-1 text-slate-600">status: {{ ex.status }} | exit: {{ ex.exitCode }} | {{ ex.durationMs }}ms</div>
              <pre v-if="ex.stdout" class="mt-2 overflow-auto rounded bg-white p-2 text-[11px]">{{ ex.stdout }}</pre>
              <pre v-if="ex.stderr" class="mt-2 overflow-auto rounded bg-rose-50 p-2 text-[11px] text-rose-800">{{ ex.stderr }}</pre>
            </div>
          </div>
        </div>
      </section>
    </main>
  </div>
</template>
