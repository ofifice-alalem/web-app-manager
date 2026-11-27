export type ShellPhase = 'checking' | 'ready' | 'recovering' | 'unavailable';

export interface ShellStatus {
  phase: ShellPhase;
  message: string;
  targetUrl: string | null;
}
