import { ref, onMounted, type Ref } from 'vue';

export function useResizableWidth(options: {
  storageKey: string;
  defaultWidth: number;
  min: number;
  max: number;
}): { width: Ref<number>; startResize: (e: MouseEvent) => void } {
  const width = ref(options.defaultWidth);

  function clamp(n: number): number {
    return Math.min(options.max, Math.max(options.min, n));
  }

  onMounted(() => {
    const saved = localStorage.getItem(options.storageKey);
    if (!saved) return;
    const parsed = Number.parseInt(saved, 10);
    if (!Number.isNaN(parsed)) {
      width.value = clamp(parsed);
    }
  });

  function startResize(e: MouseEvent): void {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = width.value;

    const onMove = (ev: MouseEvent): void => {
      width.value = clamp(startWidth + ev.clientX - startX);
    };

    const onUp = (): void => {
      localStorage.setItem(options.storageKey, String(width.value));
      document.removeEventListener('mousemove', onMove);
      document.removeEventListener('mouseup', onUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };

    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
    document.addEventListener('mousemove', onMove);
    document.addEventListener('mouseup', onUp);
  }

  return { width, startResize };
}
