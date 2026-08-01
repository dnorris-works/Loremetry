<template>
  <div class="maintenance-overlay">
    <div class="maintenance-card">
      <h1 class="maintenance-title">Platform Setup in Progress</h1>
      <p class="maintenance-message">
        The platform is being configured by the administrator.
        Please try again later.
      </p>
    </div>
  </div>
</template>

<style scoped>
.maintenance-overlay {
  position: fixed;
  inset: 0;
  z-index: 9999;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.7);
}

.maintenance-card {
  background: var(--surface, #1a1a2e);
  border: 1px solid var(--border, #333);
  border-radius: 12px;
  padding: 48px 40px;
  max-width: 440px;
  text-align: center;
}

.maintenance-title {
  font-size: 20px;
  font-weight: 700;
  color: var(--text, #eee);
  margin-bottom: 16px;
}

.maintenance-message {
  font-size: 14px;
  line-height: 1.6;
  color: var(--text-muted, #999);
}
</style>
