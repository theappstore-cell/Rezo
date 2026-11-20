import { Bluetooth, Cable, Headphones, Monitor, Plug, Usb, Volume2, VolumeX, Wifi } from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import { api } from '../lib/api';
import { t } from '../lib/i18n';
import type { DeviceKind, MixerApp, MixerDevice, MixerState } from '../lib/types';

const POLL_MS = 1000;
const HOLD_MS = 1200;

const KIND: Record<DeviceKind, { label: string; icon: ReactNode }> = {
  bluetooth: { label: 'Bluetooth', icon: <Bluetooth size={18} /> },
  wired: { label: 'Filaire (jack / intégré)', icon: <Headphones size={18} /> },
  usb: { label: 'USB', icon: <Usb size={18} /> },
  hdmi: { label: 'HDMI / DisplayPort / optique', icon: <Monitor size={18} /> },
  network: { label: 'Réseau', icon: <Wifi size={18} /> },
  other: { label: 'Autre sortie', icon: <Plug size={18} /> },
};

function Level({ value }: { value: number }) {
  const w = Math.min(1, Math.sqrt(Math.max(0, value))) * 100;
  return (
    <div className="h-[4px] w-full overflow-hidden rounded-full" style={{ background: 'var(--c-meter-off)' }} aria-hidden>
      <div className="h-full rounded-full" style={{ width: `${w}%`, background: 'var(--c-ok)' }} />
    </div>
  );
}

function VolumeRow({ label, value, mute, onVolume, onMute, disabled }: {
  label: string;
  value: number;
  mute: boolean;
  onVolume: (v: number) => void;
  onMute: () => void;
  disabled?: boolean;
}) {
  return (
    <div className="flex items-center gap-2.5">
      <button type="button" className="key key-icon grid !h-7 !w-7 shrink-0 place-items-center" data-on={mute}
        style={{ ['--c' as string]: 'var(--c-bad)' }} aria-pressed={mute} aria-label={mute ? t('Réactiver le son de {name}', { name: label }) : t('Couper le son de {name}', { name: label })}
        title={mute ? t('Son coupé : cliquer pour réactiver') : t('Couper le son')} onClick={onMute} disabled={disabled}>
        {mute ? <VolumeX size={14} /> : <Volume2 size={14} />}
      </button>
      <input type="range" min={0} max={100} step={1} value={Math.round(value * 100)} disabled={disabled}
        aria-label={t('Volume de {name}', { name: label })} onChange={(e) => onVolume(Number(e.target.value) / 100)}
        className="h-5 min-w-0 flex-1 cursor-pointer" style={{ accentColor: 'var(--c-ink)', opacity: mute ? 0.45 : 1 }} />
      <span className="num w-10 shrink-0 text-right text-[12.5px] font-semibold">{Math.round(value * 100)} %</span>
    </div>
  );
}

function appName(a: MixerApp): string {
  if (a.key === '@system') return t('Sons système Windows');
  if (a.key.startsWith('pid:')) return t('Processus {n}', { n: a.key.slice(4) });
  return a.name;
}

function AppAvatar({ app }: { app: MixerApp }) {
  if (app.icon) return <img src={app.icon} alt="" width={22} height={22} className="shrink-0" />;
  return <span className="grid h-[22px] w-[22px] shrink-0 place-items-center rounded bg-inset text-[11px] font-semibold text-mute">{appName(app).slice(0, 1).toUpperCase()}</span>;
}

export function MixerWindow() {
  const [state, setState] = useState<MixerState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loadFailed, setLoadFailed] = useState(false);
  const held = useRef(new Map<string, { volume?: number; mute?: boolean; at: number }>());

  const refresh = useCallback(async () => {
    try {
      const s = await api.mixerState();
      const now = Date.now();
      for (const [k, h] of held.current) if (now - h.at > HOLD_MS) held.current.delete(k);
      const withHeld = {
        ...s,
        devices: s.devices.map((d) => {
          const h = held.current.get(`d:${d.id}`);
          return h ? { ...d, volume: h.volume ?? d.volume, mute: h.mute ?? d.mute } : d;
        }),
        apps: s.apps.map((a) => {
          const h = held.current.get(`a:${a.deviceId}|${a.key}`);
          return h ? { ...a, volume: h.volume ?? a.volume, mute: h.mute ?? a.mute } : a;
        }),
      };
      setState(withHeld);
      setLoadFailed(false);
    } catch (e) {
      setLoadFailed(true);
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
    const t = window.setInterval(() => {
      if (document.visibilityState === 'visible') void refresh();
    }, POLL_MS);
    return () => window.clearInterval(t);
  }, [refresh]);

  const fail = (e: unknown) => setError(String(e));
  const hold = (key: string, patch: { volume?: number; mute?: boolean }) =>
    held.current.set(key, { ...held.current.get(key), ...patch, at: Date.now() });
  const patch = (fn: (s: MixerState) => MixerState) => setState((s) => (s ? fn(s) : s));

  const setDeviceVolume = (d: MixerDevice, volume: number) => {
    hold(`d:${d.id}`, { volume });
    patch((s) => ({ ...s, devices: s.devices.map((x) => (x.id === d.id ? { ...x, volume } : x)) }));
    api.mixerDeviceVolume(d.id, volume).catch(fail);
  };
  const toggleDeviceMute = (d: MixerDevice) => {
    hold(`d:${d.id}`, { mute: !d.mute });
    patch((s) => ({ ...s, devices: s.devices.map((x) => (x.id === d.id ? { ...x, mute: !d.mute } : x)) }));
    api.mixerDeviceMute(d.id, !d.mute).catch(fail);
  };
  const setAppVolume = (a: MixerApp, volume: number) => {
    hold(`a:${a.deviceId}|${a.key}`, { volume });
    patch((s) => ({ ...s, apps: s.apps.map((x) => (x.key === a.key && x.deviceId === a.deviceId ? { ...x, volume } : x)) }));
    api.mixerAppVolume(a.deviceId, a.key, volume).catch(fail);
  };
  const toggleAppMute = (a: MixerApp) => {
    hold(`a:${a.deviceId}|${a.key}`, { mute: !a.mute });
    patch((s) => ({ ...s, apps: s.apps.map((x) => (x.key === a.key && x.deviceId === a.deviceId ? { ...x, mute: !a.mute } : x)) }));
    api.mixerAppMute(a.deviceId, a.key, !a.mute).catch(fail);
  };
  const makeDefault = (d: MixerDevice) => {
    setError(null);
    api.mixerSetDefault(d.id).then(refresh).catch(fail);
  };

  const groups = useMemo(() => {
    if (!state) return [];
    return state.devices
      .map((d) => ({ device: d, apps: state.apps.filter((a) => a.deviceId === d.id) }))
      .filter((g) => g.apps.length > 0);
  }, [state]);

  return (
    <div className="flex h-full flex-col bg-chassis text-ink">
      <header className="shrink-0 border-b border-line bg-panel px-5 py-3">
        <h1 className="text-[16px] font-semibold">{t('Mixeur audio')}</h1>
        <p className="text-[12.5px] text-mute">
          {t("Sorties du PC (filaires, Bluetooth, HDMI…) et volume de chaque application. Les réglages sont ceux de Windows : ils s'appliquent aussitôt.")}
        </p>
      </header>

      {error && (
        <div role="alert" className="flex shrink-0 items-start gap-2 border-b border-line bg-panel2 px-5 py-2 text-[12.5px] text-bad">
          <span className="min-w-0 flex-1">{error}</span>
          <button type="button" className="key !h-6 shrink-0" onClick={() => setError(null)}>{t('Fermer')}</button>
        </div>
      )}

      <main className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
        {!state && !loadFailed && <p className="text-mute">{t('Lecture des périphériques audio…')}</p>}
        {!state && loadFailed && <p className="text-bad">{t('Impossible de lire les périphériques audio.')}</p>}

        {state && (
          <>
            <section aria-labelledby="sorties">
              <h2 id="sorties" className="mb-2 text-[13px] font-semibold">{t('Sorties audio')}</h2>
              {state.devices.length === 0 && <p className="text-mute">{t('Aucune sortie audio active. Branchez un casque ou connectez un appareil Bluetooth.')}</p>}
              <div className="flex flex-col gap-2">
                {state.devices.map((d) => (
                  <div key={d.id} className="rounded-md border border-line bg-panel p-3" style={{ borderLeft: d.default ? '3px solid var(--c-ok)' : undefined }}>
                    <div className="mb-2 flex items-center gap-3">
                      <span className="text-mute" title={t(KIND[d.kind].label)}>{KIND[d.kind].icon ?? <Cable size={18} />}</span>
                      <div className="min-w-0 flex-1">
                        <div className="truncate text-[13.5px] font-semibold" title={d.name}>{d.name}</div>
                        <div className="legend">{t(KIND[d.kind].label)}</div>
                      </div>
                      {d.default ? (
                        <span className="rounded border border-line px-2 py-0.5 text-[11.5px] font-semibold text-ok">{t('Sortie par défaut')}</span>
                      ) : (
                        <button type="button" className="key" onClick={() => makeDefault(d)} disabled={state.defaultLocked}
                          title={state.defaultLocked ? t("Smart Output est actif (haut-parleurs du PC coupés) : désactivez-le ou arrêtez les envois d'abord.") : t('Faire de cette sortie la sortie par défaut de Windows')}>
                          {t('Utiliser cette sortie')}
                        </button>
                      )}
                    </div>
                    <VolumeRow label={d.name} value={d.volume} mute={d.mute} onVolume={(v) => setDeviceVolume(d, v)} onMute={() => toggleDeviceMute(d)} />
                    <div className="mt-2 pl-[38px]"><Level value={d.mute ? 0 : d.peak} /></div>
                  </div>
                ))}
              </div>
              {state.defaultLocked && (
                <p className="mt-2 text-[12px] text-mute">{t('Smart Output est actif : les haut-parleurs du PC sont coupés pendant les envois, le changement de sortie par défaut est donc bloqué.')}</p>
              )}
            </section>

            <section aria-labelledby="applis" className="mt-6">
              <h2 id="applis" className="mb-2 text-[13px] font-semibold">{t('Applications')}</h2>
              {groups.length === 0 && <p className="text-mute">{t("Aucune application n'utilise le son pour l'instant.")}</p>}
              <div className="flex flex-col gap-4">
                {groups.map(({ device, apps }) => (
                  <div key={device.id}>
                    <div className="legend mb-1.5 flex items-center gap-1.5">
                      <span className="text-mute">{KIND[device.kind].icon}</span>
                      <span>{t('Sur : {name}', { name: device.name })}</span>
                    </div>
                    <div className="flex flex-col divide-y divide-line rounded-md border border-line bg-panel">
                      {apps.map((a) => (
                        <div key={a.key} className="px-3 py-2.5">
                          <div className="mb-1.5 flex items-center gap-2.5">
                            <AppAvatar app={a} />
                            <span className="min-w-0 flex-1 truncate text-[13px] font-medium" title={a.key}>{appName(a)}</span>
                            {a.active && <span className="h-[7px] w-[7px] rounded-full bg-ok" title={t('Joue en ce moment')} />}
                          </div>
                          <VolumeRow label={appName(a)} value={a.volume} mute={a.mute} onVolume={(v) => setAppVolume(a, v)} onMute={() => toggleAppMute(a)} />
                          <div className="mt-1.5 pl-[38px]"><Level value={a.mute ? 0 : a.peak} /></div>
                        </div>
                      ))}
                    </div>
                  </div>
                ))}
              </div>
              <p className="mt-3 text-[12px] leading-snug text-dim">
                {t('Pour envoyer une application précise vers une autre sortie, utilisez Paramètres Windows › Système › Son › Mélangeur de volume : Windows ne laisse pas une application tierce le faire de façon fiable.')}
              </p>
            </section>
          </>
        )}
      </main>
    </div>
  );
}
