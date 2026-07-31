import { ref, computed } from 'vue';

export type PlatformId = 'kdp' | 'craft' | 'publish' | 'saved';

const stored = localStorage.getItem('platform');
const platform = ref<PlatformId>(
  stored === 'wide' ? 'kdp' : (stored as PlatformId) || 'kdp',
);

const isKdp = computed(() => platform.value === 'kdp');

function setPlatform(p: PlatformId): void {
  platform.value = p;
  localStorage.setItem('platform', p);
}

export function usePlatform() {
  return {
    platform,
    isKdp,
    setPlatform,
  };
}
