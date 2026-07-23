import { createApp } from 'vue';
import { clerkPlugin } from '@clerk/vue';
import App from './App.vue';
import './style.css';

async function boot(): Promise<void> {
  let publishableKey = '';
  let clerkEnabled = false;
  try {
    const res = await fetch('/api/auth/config');
    const data = await res.json();
    clerkEnabled = Boolean(data.clerkEnabled);
    publishableKey = data.publishableKey ?? '';
  } catch {
    /* local dev without API */
  }

  const app = createApp(App, { clerkEnabled, publishableKey });

  if (clerkEnabled && publishableKey) {
    app.use(clerkPlugin, { publishableKey });
  }

  app.config.errorHandler = (err, _instance, info) => {
    console.error('[Vue Error]', info, err);
    const el = document.getElementById('error-toast');
    if (el) {
      el.textContent = `Error: ${err instanceof Error ? err.message : String(err)}`;
      el.style.display = 'block';
      setTimeout(() => { el.style.display = 'none'; }, 5000);
    }
  };

  app.mount('#app');
}

void boot();
