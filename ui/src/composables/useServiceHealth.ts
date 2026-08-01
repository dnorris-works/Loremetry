import { ref } from 'vue';
import { buildAuthHeaders } from '../api';

export interface ServiceStatus {
  configured: boolean;
  connected: boolean;
  error: string | null;
  checking: boolean;
}

export interface ServiceHealthState {
  ai: ServiceStatus;
  canopy: ServiceStatus;
  dataforseo: ServiceStatus;
}

function defaultStatus(): ServiceStatus {
  return { configured: false, connected: false, error: null, checking: true };
}

const serviceHealth = ref<ServiceHealthState>({
  ai: defaultStatus(),
  canopy: defaultStatus(),
  dataforseo: defaultStatus(),
});

async function checkHealth(): Promise<void> {
  // Set all to checking
  serviceHealth.value = {
    ai: { ...serviceHealth.value.ai, checking: true },
    canopy: { ...serviceHealth.value.canopy, checking: true },
    dataforseo: { ...serviceHealth.value.dataforseo, checking: true },
  };

  try {
    const headers = await buildAuthHeaders({ 'Content-Type': 'application/json' });
    const res = await fetch('/api/health/services', { headers });
    if (!res.ok) {
      throw new Error(`HTTP ${res.status}`);
    }
    const data = await res.json() as {
      ai: { configured: boolean; connected: boolean; error: string | null };
      canopy: { configured: boolean; connected: boolean; error: string | null };
      dataforseo: { configured: boolean; connected: boolean; error: string | null };
    };
    serviceHealth.value = {
      ai: { ...data.ai, checking: false },
      canopy: { ...data.canopy, checking: false },
      dataforseo: { ...data.dataforseo, checking: false },
    };
  } catch (e) {
    // Network failure — mark all as unknown
    const err = e instanceof Error ? e.message : String(e);
    serviceHealth.value = {
      ai: { configured: false, connected: false, error: err, checking: false },
      canopy: { configured: false, connected: false, error: err, checking: false },
      dataforseo: { configured: false, connected: false, error: err, checking: false },
    };
  }
}

export function useServiceHealth() {
  return { serviceHealth, checkHealth };
}
