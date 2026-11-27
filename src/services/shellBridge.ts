import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { ShellStatus } from '../types/shell';

export const SHELL_STATUS_EVENT = 'shell-status-changed';

/**
 * Typed IPC boundary for communicating with the Rust backend shell.
 * The frontend never executes system operations directly.
 */
export const shellBridge = {
  async getStatus(): Promise<ShellStatus> {
    return invoke<ShellStatus>('get_shell_status');
  },

  async retryConnection(): Promise<ShellStatus> {
    return invoke<ShellStatus>('retry_shell_connection');
  },

  async reloadWebview(): Promise<void> {
    return invoke<void>('reload_shell_webview');
  },

  async onStatusChanged(handler: (status: ShellStatus) => void): Promise<UnlistenFn> {
    return listen<ShellStatus>(SHELL_STATUS_EVENT, (event) => {
      handler(event.payload);
    });
  },
};
