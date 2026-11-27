import { describe, it, expect, vi, beforeEach } from 'vitest';
import { defineComponent } from 'vue';
import { flushPromises, mount } from '@vue/test-utils';
import { useShellState } from '../useShellState';
import { shellBridge } from '../../services/shellBridge';
import type { ShellStatus } from '../../types/shell';

vi.mock('../../services/shellBridge', () => ({
  shellBridge: {
    getStatus: vi.fn(),
    retryConnection: vi.fn(),
    reloadWebview: vi.fn(),
    onStatusChanged: vi.fn(),
  },
}));

describe('useShellState', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('subscribes to status events and cleans up listener on unmount', async () => {
    const unlistenMock = vi.fn();
    let capturedHandler: ((status: ShellStatus) => void) | undefined;

    vi.mocked(shellBridge.onStatusChanged).mockImplementation(async (handler) => {
      capturedHandler = handler;
      return unlistenMock;
    });
    vi.mocked(shellBridge.getStatus).mockResolvedValue({
      phase: 'checking',
      message: 'Checking...',
      targetUrl: null,
    });

    const TestComp = defineComponent({
      setup() {
        return useShellState();
      },
      template: '<div>{{ status.phase }}</div>',
    });

    const wrapper = mount(TestComp);
    await flushPromises();

    expect(wrapper.text()).toBe('checking');

    capturedHandler?.({
      phase: 'ready',
      message: 'Ready',
      targetUrl: 'http://127.0.0.1:8000',
    });
    await flushPromises();

    expect(wrapper.text()).toBe('ready');

    wrapper.unmount();
    expect(unlistenMock).toHaveBeenCalledTimes(1);
  });
});
