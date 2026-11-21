import { useEffect } from 'react';
import { AddDeviceModal, RoomEditor, SettingsModal, SpeakerSheet } from './components/Dialogs';
import { TopBar } from './components/TopBar';
import { UpdateBanner } from './components/UpdateBanner';
import { useMidi } from './lib/midi';
import { useStore } from './lib/store';
import { t, useLang } from './lib/i18n';
import { useUi } from './lib/ui';
import { ConsoleView } from './views/ConsoleView';
import { PlanView } from './views/PlanView';
import { RoomsView } from './views/RoomsView';

function Toast() {
  const toast = useStore((s) => s.toast);
  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => useStore.setState({ toast: null }), 4500);
    return () => window.clearTimeout(timer);
  }, [toast]);
  if (!toast) return null;
  return (
    <div role="status" className="toast-in pointer-events-none fixed inset-x-0 bottom-10 z-50 flex justify-center px-4">
      <div className="max-w-[420px] rounded-md bg-ink px-4 py-2.5 text-[13px] text-onsig" style={{ boxShadow: '0 6px 18px rgba(0,0,0,0.3)' }}>{toast}</div>
    </div>
  );
}

function Modals() {
  const modal = useUi((u) => u.modal);
  if (!modal) return null;
  switch (modal.type) {
    case 'add': return <AddDeviceModal />;
    case 'settings': return <SettingsModal />;
    case 'speaker': return <SpeakerSheet id={modal.id} />;
    case 'room': return <RoomEditor id={modal.id} />;
  }
}

function StatusBar() {
  const s = useStore((st) => st.s);
  if (!s) return null;
  const active = s.speakers.filter((sp) => sp.running).length;
  const streaming = s.speakers.filter((sp) => sp.streaming).length;
  const source = s.source.kind === 'system' ? t('tout le son du PC') : s.source.name;
  return (
    <footer className="flex shrink-0 items-center gap-4 whitespace-nowrap border-t border-line bg-panel px-3 py-1.5 text-[12px] text-mute">
      <span className="flex items-center gap-1.5" title={s.capture.running ? t('Capture : {source}', { source }) : s.capture.error ?? t('Aucune enceinte active : Rezo ne capte rien et laisse les périphériques audio au repos')}>
        <span className="h-[8px] w-[8px] rounded-full" style={{ background: s.capture.running ? "var(--c-ok)" : s.capture.error ? "var(--c-bad)" : "var(--c-border-strong)" }} />
        {s.capture.running ? t('Capture active') : s.capture.error ? t('Capture en erreur') : t('En veille')}
      </span>
      <span className="num" title={t('{n} reçoivent effectivement le flux', { n: streaming })}>{t(active > 1 ? '{n} enceintes en service' : '{n} enceinte en service', { n: active })}</span>
      <span className="ml-auto" title={t("Smart Output : coupe la sortie du PC tant qu'une enceinte réseau est active")}>{s.pcMuted ? t('PC coupé') : t('PC audible')}</span>
    </footer>
  );
}

export default function App() {
  const ready = useStore((s) => s.s !== null);
  const view = useStore((s) => s.view);
  const learning = useMidi((m) => m.learn);
  const init = useStore((s) => s.init);
  const lang = useLang((l) => l.lang);

  useEffect(() => {
    void init();
  }, [init]);

  if (!ready) {
    return <div className="grid h-full place-items-center text-[13px] text-mute">{t('Démarrage du moteur audio…')}</div>;
  }
  return (
    <div key={lang} className={`flex h-full flex-col ${learning ? 'learning' : ''}`}>
      <TopBar />
      <UpdateBanner />
      <main className="min-h-0 flex-1">
        {view === 'rooms' && <RoomsView />}
        {view === 'plan' && <PlanView />}
        {view === 'console' && <ConsoleView />}
      </main>
      <StatusBar />
      <Modals />
      <Toast />
    </div>
  );
}
