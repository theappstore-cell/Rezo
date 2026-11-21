import { useEffect, useState } from 'react';
import { Download, X } from 'lucide-react';
import { api, on } from '../lib/api';
import { useStore } from '../lib/store';
import { t } from '../lib/i18n';
import type { UpdateInfo } from '../lib/types';

interface Progress {
  received: number;
  total: number;
  stage: 'download' | 'verify' | 'restart';
}

const DISMISS_KEY = 'rezo.update.dismissed';
const RECHECK_MS = 6 * 3600 * 1000;

const mb = (n: number) => (n / 1048576).toFixed(1);

function dismissed(version: string): boolean {
  try {
    return localStorage.getItem(DISMISS_KEY) === version;
  } catch {
    return false;
  }
}

// Shown when GitHub has a newer release; "Install" downloads it into Rezo's own folder, checks it and restarts.
export function UpdateBanner() {
  const enabled = useStore((s) => s.s?.settings.checkUpdates ?? false);
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!enabled) return;
    let alive = true;
    const run = () =>
      api
        .checkUpdate()
        .then((i) => {
          if (alive && i.available && !dismissed(i.latest)) setInfo(i);
        })
        .catch(() => {});
    void run();
    const timer = window.setInterval(run, RECHECK_MS);
    return () => {
      alive = false;
      window.clearInterval(timer);
    };
  }, [enabled]);

  useEffect(() => {
    const show = (e: Event) => setInfo((e as CustomEvent<UpdateInfo>).detail);
    window.addEventListener('rezo-update', show);
    return () => window.removeEventListener('rezo-update', show);
  }, []);

  useEffect(() => {
    let off: (() => void) | undefined;
    void on<Progress>('update-progress', setProgress).then((u) => (off = u));
    return () => off?.();
  }, []);

  if (!info) return null;

  const install = async () => {
    setBusy(true);
    setError(null);
    setProgress({ received: 0, total: info.size ?? 0, stage: 'download' });
    try {
      await api.installUpdate();
    } catch (e) {
      setError(String(e));
      setProgress(null);
      setBusy(false);
    }
  };
  const later = () => {
    try {
      localStorage.setItem(DISMISS_KEY, info.latest);
    } catch {}
    setInfo(null);
  };

  const pct = progress && progress.total > 0 ? Math.min(100, (progress.received / progress.total) * 100) : 0;
  const status = !progress
    ? t('Rezo {latest} est disponible (version installée : {current}).', { latest: info.latest, current: info.current })
    : progress.stage === 'download'
      ? t('Téléchargement de Rezo {latest} : {done} / {total} Mo', { latest: info.latest, done: mb(progress.received), total: mb(progress.total) })
      : progress.stage === 'verify'
        ? t('Vérification de la somme SHA-256…')
        : t('Installation terminée, redémarrage…');

  return (
    <div role="status" className="shrink-0 border-b border-line bg-panel2 px-3 py-2">
      <div className="flex items-center gap-2">
        <Download size={14} className="shrink-0 text-ok" aria-hidden />
        <span className="min-w-0 flex-1 text-[12.5px]">{status}</span>
        {!busy && info.canInstall && (
          <button type="button" className="key key-primary !h-7 !text-[12px]" onClick={() => void install()} title={t('Téléchargé dans {folder}, vérifié puis installé à la place de la version actuelle.', { folder: info.folder })}>
            {t('Télécharger et installer')}
          </button>
        )}
        {!busy && !info.canInstall && (
          <button type="button" className="key key-primary !h-7 !text-[12px]" onClick={() => void api.openReleasePage()}>
            {t('Ouvrir la page de téléchargement')}
          </button>
        )}
        {!busy && (
          <button type="button" className="rounded p-1 text-dim hover:bg-raise hover:text-ink" aria-label={t('Plus tard')} title={t('Plus tard')} onClick={later}>
            <X size={14} />
          </button>
        )}
      </div>
      {progress && (
        <div className="mt-1.5 h-[6px] overflow-hidden rounded-full" style={{ background: 'var(--c-meter-off)' }} role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(pct)}>
          <div className="h-full rounded-full bg-ok transition-[width] duration-150" style={{ width: `${progress.stage === 'download' ? pct : 100}%` }} />
        </div>
      )}
      {error && <div className="mt-1.5 text-[12px] text-bad">{error}</div>}
    </div>
  );
}

export async function checkNow(): Promise<string> {
  try {
    const i = await api.checkUpdate();
    if (!i.available) return t('Rezo est à jour (version {current}).', { current: i.current });
    try {
      localStorage.removeItem(DISMISS_KEY);
    } catch {}
    window.dispatchEvent(new CustomEvent('rezo-update', { detail: i }));
    return t('Rezo {latest} est disponible : le bandeau en haut de la fenêtre propose de l’installer.', { latest: i.latest });
  } catch (e) {
    return t('Vérification impossible : {error}', { error: String(e) });
  }
}
