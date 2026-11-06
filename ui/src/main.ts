import { mount } from 'svelte';

import App from './App.svelte';
import { app } from './lib/stores/app.svelte';
import './styles/global.scss';

// Niente menu contestuale del browser (il nostro arriva col clic destro sui suoni) né zoom con Ctrl+rotellina.
window.addEventListener('contextmenu', (e) => {
  const t = e.target as HTMLElement;
  if (!t.closest('input, textarea')) e.preventDefault();
});
window.addEventListener('wheel', (e) => e.ctrlKey && e.preventDefault(), { passive: false });

// Solo in sviluppo: lo stato è ispezionabile dagli strumenti di debug della WebView.
if (import.meta.env.DEV) (window as unknown as { __app: typeof app }).__app = app;

export default mount(App, { target: document.getElementById('app')! });
