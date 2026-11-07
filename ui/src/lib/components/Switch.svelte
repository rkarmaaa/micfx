<script lang="ts">
  let {
    checked,
    label,
    disabled = false,
    onchange,
  }: { checked: boolean; label: string; disabled?: boolean; onchange: (v: boolean) => void } = $props();
</script>

<button
  type="button"
  role="switch"
  class="switch"
  aria-checked={checked}
  aria-label={label}
  title={label}
  {disabled}
  onclick={() => onchange(!checked)}
>
  <span class="knob"></span>
</button>

<style>
  .switch {
    position: relative;
    flex: none;
    width: 36px;
    min-width: 36px;
    height: 20px;
    padding: 0;
    border: none;
    border-radius: 999px;
    background: var(--switch-off);
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.04);
    cursor: default;
    transition: background var(--t-med) var(--ease);
  }
  .switch:hover:not(:disabled) {
    background: color-mix(in srgb, var(--switch-off) 80%, var(--fg));
    border: none;
  }
  .switch[aria-checked='true'],
  .switch[aria-checked='true']:hover:not(:disabled) {
    background: var(--primary);
  }
  .switch[aria-checked='true']:hover:not(:disabled) {
    background: var(--primary-hover);
  }
  .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--knob);
    box-shadow:
      0 1px 2px rgb(0 0 0 / 0.25),
      0 0 0 0.5px rgb(0 0 0 / 0.06);
    transition: transform var(--t-med) var(--ease);
  }
  .switch[aria-checked='true'] .knob {
    transform: translateX(16px);
  }
  .switch:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
