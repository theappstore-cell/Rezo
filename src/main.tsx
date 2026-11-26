import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { getCurrentWindow } from '@tauri-apps/api/window';
import App from './App';
import { MixerWindow } from './views/MixerWindow';
import { WidgetWindow } from './views/WidgetWindow';
import { isTauri } from './lib/api';
import { syncLangWithBackend, useLang } from './lib/i18n';
import './index.css';

window.addEventListener('contextmenu', (e) => {
  if (!(e.target instanceof HTMLInputElement)) e.preventDefault();
});

const label = isTauri ? getCurrentWindow().label : '';
const isMixer = isTauri ? label === 'mixer' : window.location.hash === '#mixer';
// Desktop widgets are windows labelled "widget-<id>" (in a browser preview: #widget=<id>).
const widgetId = isTauri
  ? (label.startsWith('widget-') ? label.slice('widget-'.length) : null)
  : (/^#widget=(\w+)$/.exec(window.location.hash)?.[1] ?? null);
if (widgetId) document.documentElement.classList.add('widget-window');
syncLangWithBackend();

function Mixer() {
  const lang = useLang((l) => l.lang);
  return <MixerWindow key={lang} />;
}

function Widget({ id }: { id: string }) {
  const lang = useLang((l) => l.lang);
  return <WidgetWindow key={lang} id={id} />;
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>{widgetId ? <Widget id={widgetId} /> : isMixer ? <Mixer /> : <App />}</StrictMode>,
);
