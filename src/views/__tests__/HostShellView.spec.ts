import { describe, it, expect, vi, beforeEach } from 'vitest';
import { flushPromises, mount } from '@vue/test-utils';
import HostShellView from '../HostShellView.vue';
import { shellBridge } from '../../services/shellBridge';

vi.mock('../../services/shellBridge', () => ({
  shellBridge: {
    getStatus: vi.fn(),
    retryConnection: vi.fn(),
    reloadWebview: vi.fn(),
    onStatusChanged: vi.fn(),
  },
}));

describe('HostShellView', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(shellBridge.onStatusChanged).mockResolvedValue(() => {});
  });

  it('renders loading indicator while checking application readiness', async () => {
    vi.mocked(shellBridge.getStatus).mockResolvedValue({
      phase: 'checking',
      message: 'Checking application readiness...',
      targetUrl: null,
    });

    const wrapper = mount(HostShellView);
    await flushPromises();

    expect(wrapper.find('[role="status"]').exists()).toBe(true);
    expect(wrapper.text()).toContain('Checking application readiness...');
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
  });

  it('renders generic error fallback and triggers retry via IPC', async () => {
    vi.mocked(shellBridge.getStatus).mockResolvedValue({
      phase: 'unavailable',
      message: 'Application could not be reached.',
      targetUrl: null,
    });
    vi.mocked(shellBridge.retryConnection).mockResolvedValue({
      phase: 'checking',
      message: 'Checking application readiness...',
      targetUrl: null,
    });

    const wrapper = mount(HostShellView);
    await flushPromises();

    expect(wrapper.find('[role="alert"]').exists()).toBe(true);
    expect(wrapper.text()).toContain('Application Unavailable');
    expect(wrapper.text()).toContain('Application could not be reached.');

    await wrapper.find('button').trigger('click');
    await flushPromises();

    expect(shellBridge.retryConnection).toHaveBeenCalledTimes(1);
    expect(wrapper.find('[role="status"]').exists()).toBe(true);
  });

  it('falls back to generic unavailable state when IPC fails', async () => {
    vi.mocked(shellBridge.getStatus).mockRejectedValue(new Error('IPC disconnected'));

    const wrapper = mount(HostShellView);
    await flushPromises();

    expect(wrapper.find('[role="alert"]').exists()).toBe(true);
    expect(wrapper.text()).not.toContain('IPC disconnected');
  });
});
