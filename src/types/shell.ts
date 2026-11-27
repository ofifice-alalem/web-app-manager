export type ShellPhase = 'checking' | 'ready' | 'unavailable';

export interface ShellStatus {
  phase: ShellPhase;
  message: string;
  targetUrl: string | null;
}
