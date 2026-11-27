import { describe, it, expect } from 'vitest';
import appJson from '../../../config/app.json';
import servicesJson from '../../../config/services.json';
import rulesJson from '../../../config/recovery-rules.json';
import appSchema from '../../../config/schemas/app.schema.json';
import servicesSchema from '../../../config/schemas/services.schema.json';
import rulesSchema from '../../../config/schemas/recovery-rules.schema.json';

describe('Default JSON configuration files and schemas', () => {
  it('parses app.json, services.json, and recovery-rules.json cleanly', () => {
    expect(appJson.application.id).toBe('sales-system');
    expect(appJson.application.startup_timeout_seconds).toBeGreaterThan(0);

    const serviceIds = new Set(servicesJson.services.map((s) => s.id));
    expect(serviceIds.has('sales-app')).toBe(true);
    expect(serviceIds.has('print-server')).toBe(true);

    for (const rule of rulesJson.rules) {
      expect(serviceIds.has(rule.trigger.service_id)).toBe(true);
      expect(rule.retry.max_attempts).toBeGreaterThanOrEqual(1);
    }
  });

  it('parses all JSON Schema files in config/schemas/', () => {
    for (const schema of [appSchema, servicesSchema, rulesSchema]) {
      expect(schema.$schema).toContain('json-schema.org');
    }
  });
});
