/**
 * Sanitized configuration DTOs exposed to the Vue client shell.
 * Never includes system command definitions, paths, or secrets.
 */

export interface PublicApplicationInfo {
  id: string;
  name: string;
  url: string;
  startupTimeoutSeconds: number;
  reloadAfterRecovery: boolean;
}

export interface ConfigDiagnosticSummary {
  hasErrors: boolean;
  diagnosticCount: number;
}
