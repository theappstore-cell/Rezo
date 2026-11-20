import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { getCurrentWindow } from '@tauri-apps/api/window';
import App from './App';
import { MixerWindow } from './views/MixerWindow';
import { isTauri } from './lib/api';
import { syncLangWithBackend, useLang } from './lib/i18n';
import './index.css';

window.addEventListener('contextmenu', (e) => {
  if (!(e.target instanceof HTMLInputElement)) e.preventDefault();
});

const isMixer = isTauri ? getCurrentWindow().label === 'mixer' : window.location.hash === '#mixer';
syncLangWithBackend();

function Mixer() {
  const lang = useLang((l) => l.lang);
  return <MixerWindow key={lang} />;
}

createRoot(document.getElementById('root')!).render(<StrictMode>{isMixer ? <Mixer /> : <App />}</StrictMode>);
