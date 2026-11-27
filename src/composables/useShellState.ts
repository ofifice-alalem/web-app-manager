import { computed, onMounted, onUnmounted, ref } from 'vue';
import type { UnlistenFn } from '@tauri-apps/api/event';
import { shellBridge } from '../services/shellBridge';
import type { ShellStatus } from '../types/shell';

const GENERIC_UNAVAILABLE_MESSAGE =
  'The application could not be reached. Please verify the system is available and try again.';

export function useShellState() {
  const status = ref<ShellStatus>({
    phase: 'checking',
    message: 'Preparing application...',
    targetUrl: null,
  });

  const isLoading = computed(
    () => status.value.phase === 'checking' || status.value.phase === 'recovering',
  );
  const isReady = computed(() => status.value.phase === 'ready');
  const isUnavailable = computed(() => status.value.phase === 'unavailable');

  let unlistenStatus: UnlistenFn | null = null;

  async function refreshStatus(): Promise<void> {
    try {
      status.value = await shellBridge.getStatus();
    } catch {
      status.value = {
        phase: 'unavailable',
        message: GENERIC_UNAVAILABLE_MESSAGE,
        targetUrl: null,
      };
    }
  }

  async function retryConnection(): Promise<void> {
    try {
      status.value = await shellBridge.retryConnection();
    } catch {
      status.value = {
        phase: 'unavailable',
        message: GENERIC_UNAVAILABLE_MESSAGE,
        targetUrl: status.value.targetUrl,
      };
    }
  }

  onMounted(async () => {
    try {
      unlistenStatus = await shellBridge.onStatusChanged((nextStatus) => {
        status.value = nextStatus;
      });
    } catch {
      // Event listener registration can fail outside Tauri runtime (e.g., plain browser preview).
    }
    await refreshStatus();
  });

  onUnmounted(() => {
    if (unlistenStatus) {
      unlistenStatus();
      unlistenStatus = null;
    }
  });

  return {
    status,
    isLoading,
    isReady,
    isUnavailable,
    refreshStatus,
    retryConnection,
  };
}
