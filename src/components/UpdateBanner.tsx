import { useEffect, useState } from 'react';
import type { ReactNode } from 'react';
import { ChevronDown, Download, X } from 'lucide-react';
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

// **bold**, `code` and [links](…) of the release notes, as plain React text (links show their label only).
function inline(text: string): ReactNode[] {
  const out: ReactNode[] = [];
  const re = /\*\*([^*]+)\*\*|`([^`]+)`|\[([^\]]+)\]\([^)]*\)/g;
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text))) {
    if (m.index > last) out.push(text.slice(last, m.index));
    if (m[1]) out.push(<b key={m.index}>{m[1]}</b>);
    else if (m[2]) out.push(<code key={m.index} className="rounded bg-inset px-1 font-mono text-[11px]">{m[2]}</code>);
    else out.push(m[3]);
    last = re.lastIndex;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

// The release notes (Markdown headings, bullets and paragraphs) of the new version.
export function ReleaseNotes({ text }: { text: string }) {
  const blocks: ReactNode[] = [];
  let items: string[] = [];
  const flush = () => {
    if (items.length) {
      const list = items;
      blocks.push(<ul key={blocks.length} className="ml-4 list-disc space-y-0.5">{list.map((it, i) => <li key={i}>{inline(it)}</li>)}</ul>);
      items = [];
    }
  };
  for (const raw of text.split('\n')) {
    const line = raw.trimEnd();
    const h = /^#{1,6}\s+(.*)$/.exec(line);
    const li = /^\s*[-*]\s+(.*)$/.exec(line);
    if (h) {
      flush();
      blocks.push(<div key={blocks.length} className="mt-1.5 font-semibold first:mt-0">{inline(h[1])}</div>);
    } else if (li) items.push(li[1]);
    else if (!line.trim()) flush();
    else {
      flush();
      blocks.push(<p key={blocks.length}>{inline(line)}</p>);
    }
  }
  flush();
  return <div className="space-y-1 text-[12px] leading-snug">{blocks}</div>;
}

// Shown when GitHub has a newer release; "Install" downloads it into Rezo's own folder, checks it and restarts.
export function UpdateBanner() {
  const enabled = useStore((s) => s.s?.settings.checkUpdates ?? false);
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [showNotes, setShowNotes] = useState(false);

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
        {!busy && !!info.notes && (
          <button type="button" className="flex shrink-0 items-center gap-1 rounded px-1.5 py-1 text-[12px] text-mute hover:bg-raise hover:text-ink" aria-expanded={showNotes} onClick={() => setShowNotes((v) => !v)}>
            {t('Nouveautés')} <ChevronDown size={13} className={showNotes ? 'rotate-180' : ''} />
          </button>
        )}
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
      {showNotes && info.notes && (
        <div className="mt-2 max-h-[220px] overflow-y-auto rounded border border-line bg-panel p-2.5">
          <ReleaseNotes text={info.notes} />
        </div>
      )}
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
