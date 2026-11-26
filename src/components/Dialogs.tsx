import { BellRing, ClipboardPaste, Copy, Keyboard, Loader2, Minus, Plus, QrCode, RefreshCw, Search, Trash2 } from 'lucide-react';
import { useEffect, useState } from 'react';
import type { ReactNode } from 'react';
import { api } from '../lib/api';
import { fmtMs } from '../lib/db';
import { ROOM_COLORS, ROOM_ICONS, SPEAKER_ICONS, protocolBadge, protocolIcon } from '../lib/icons';
import { roomLabel, useStore } from '../lib/store';
import { checkNow } from './UpdateBanner';
import { setLang, t, useLang } from '../lib/i18n';
import { isWindows } from '../lib/platform';
import { useUi } from '../lib/ui';
import type { ChannelMode, ReceiverInfo, Settings } from '../lib/types';
import { Modal, Toggle } from './Modal';
import { RotaryKnob } from './RotaryKnob';
import { NetBars, netText } from './NetBars';
import { WidgetSettings } from './WidgetSettings';
import { ZOOM_STEP } from './Shortcuts';

const inputCls = 'field';
const btnPrimary = 'key key-primary !h-auto !px-3.5 !py-2';
const btnGhost = 'key !h-auto !px-3.5 !py-2';

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="block">
      <span className="legend mb-1.5 block">{label}</span>
      {children}
    </label>
  );
}

function QrPanel() {
  const [info, setInfo] = useState<ReceiverInfo | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [ip, setIp] = useState<string | undefined>();

  useEffect(() => {
    api.getReceiverInfo(ip).then(setInfo).catch((e) => setErr(String(e)));
  }, [ip]);

  if (err) return <p className="text-[12px] text-bad">{err}</p>;
  if (!info) return <p className="text-[12px] text-mute">{t('Génération du QR code…')}</p>;
  return (
    <div className="flex flex-col items-center gap-3 text-center">
      <div className="h-[220px] w-[220px] overflow-hidden rounded-[2px] bg-white p-1" dangerouslySetInnerHTML={{ __html: info.qrSvg }} role="img" aria-label={`QR code ${info.url}`} />
      <p className="text-[12px] text-mute">{t('Scannez avec un téléphone ou une tablette')} <b className="text-ink">{t('connecté au même Wi-Fi')}</b> {t(': il devient une enceinte du PC, avec sa propre tranche de mixage.')}</p>
      <code className="select-text break-all rounded bg-inset px-2 py-1 font-mono text-[11px] text-sig">{info.url}</code>
      {info.ips.length > 1 && (
        <Field label={t('Interface réseau')}>
          <select className={inputCls} value={ip ?? info.ips[0]} onChange={(e) => setIp(e.target.value)}>
            {info.ips.map((a) => (
              <option key={a} value={a}>{a}</option>
            ))}
          </select>
        </Field>
      )}
      <p className="text-[10.5px] text-dim">{t('Pare-feu Windows : autorisez Rezo sur les réseaux privés (voir le guide).')}</p>
    </div>
  );
}

export function AddDeviceModal() {
  const close = useUi((u) => u.close);
  const s = useStore((st) => st.s);
  const scan = useStore((st) => st.scan);
  const scanning = useStore((st) => st.scanning);
  const apply = useStore((st) => st.apply);
  const [tab, setTab] = useState<'scan' | 'ip' | 'qr'>('scan');
  const [ip, setIp] = useState('');
  const [kind, setKind] = useState<'upnp' | 'native'>('upnp');
  const [port, setPort] = useState('5004');
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const add = async () => {
    setBusy(true);
    setErr(null);
    try {
      apply(await api.addManualDevice(ip.trim(), kind, kind === 'native' ? Number(port) || 5004 : undefined));
      close();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const tabs = [
    { id: 'scan' as const, label: 'Scanner', Icon: Search },
    { id: 'ip' as const, label: 'Par adresse IP', Icon: RefreshCw },
    { id: 'qr' as const, label: 'Téléphone (QR)', Icon: QrCode },
  ];

  return (
    <Modal title={t('Ajouter un appareil')} onClose={close} width={430}>
      <div role="tablist" className="mb-4 flex border-b border-line">
        {tabs.map(({ id, label, Icon }) => (
          <button key={id} type="button" role="tab" aria-selected={tab === id} onClick={() => setTab(id)}
            className={`flex flex-1 items-center justify-center gap-1.5 border-b-[3px] py-2 text-[11px] font-semibold transition ${tab === id ? 'border-accent text-ink' : 'border-transparent text-dim hover:text-ink'}`}>
            <Icon size={12} /> {t(label)}
          </button>
        ))}
      </div>

      {tab === 'scan' && (
        <div className="space-y-3">
          <button type="button" className={`${btnPrimary} flex w-full items-center justify-center gap-2`} disabled={scanning} onClick={() => void scan()}>
            {scanning ? <Loader2 size={14} className="animate-spin" /> : <Search size={14} />}
            {scanning ? t('Recherche en cours…') : t('Scanner Sonos / UPnP / mDNS')}
          </button>
          <ul className="space-y-1.5">
            {s?.speakers.filter((sp) => sp.protocol !== 'web').map((sp) => {
              const Icon = protocolIcon(sp.protocol);
              return (
                <li key={sp.id} className="flex items-center gap-2.5 rounded-[2px] border border-line bg-panel2 px-3 py-2">
                  <Icon size={15} className="text-sig" />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-[12.5px]">{sp.name}</span>
                    <span className="block truncate font-mono text-[10px] text-dim">{sp.address}{sp.model ? ` · ${sp.model}` : ''}</span>
                  </span>
                  <span className="rounded bg-inset px-1.5 py-0.5 font-mono text-[9.5px] text-sig">{protocolBadge(sp)}</span>
                  <span className={`h-[7px] w-[7px] ${sp.online ? 'bg-ok' : 'bg-bad'}`} title={sp.online ? t('en ligne') : t('hors ligne')} />
                </li>
              );
            })}
            {s && s.speakers.filter((sp) => sp.protocol !== 'web').length === 0 && <li className="text-[12px] text-mute">{t('Aucun appareil trouvé. Vérifiez que le PC et les enceintes sont sur le même réseau (et que le pare-feu autorise Rezo).')}</li>}
          </ul>
        </div>
      )}

      {tab === 'ip' && (
        <div className="space-y-3">
          <Field label={t("Type d'appareil")}>
            <select className={inputCls} value={kind} onChange={(e) => setKind(e.target.value as 'upnp' | 'native')}>
              <option value="upnp">Sonos / UPnP / DLNA</option>
              <option value="native">{t('Récepteur Rezo (UDP Opus)')}</option>
            </select>
          </Field>
          <div className="flex gap-2">
            <div className="flex-1"><Field label={t('Adresse IP')}><input className={inputCls} placeholder="192.168.1.42" value={ip} onChange={(e) => setIp(e.target.value)} inputMode="decimal" /></Field></div>
            {kind === 'native' && <div className="w-24"><Field label="Port"><input className={inputCls} value={port} onChange={(e) => setPort(e.target.value)} inputMode="numeric" /></Field></div>}
          </div>
          {err && <p className="rounded bg-bad/10 px-2 py-1.5 text-[11.5px] text-bad">{err}</p>}
          <button type="button" className={`${btnPrimary} w-full`} disabled={busy || !ip.trim()} onClick={() => void add()}>
            {busy ? t('Connexion…') : t('Ajouter')}
          </button>
        </div>
      )}

      {tab === 'qr' && <QrPanel />}
    </Modal>
  );
}

const MODES: Array<{ id: ChannelMode; label: string }> = [
  { id: 'stereo', label: 'Stéréo' },
  { id: 'mono', label: 'Mono L+R' },
  { id: 'left', label: 'Gauche seul' },
  { id: 'right', label: 'Droite seul' },
];

export function SpeakerSheet({ id }: { id: string }) {
  const close = useUi((u) => u.close);
  const s = useStore((st) => st.s);
  const setParam = useStore((st) => st.setParam);
  const apply = useStore((st) => st.apply);
  const sp = s?.speakers.find((x) => x.id === id);
  const identify = useStore((st) => st.identify);
  const reconnect = useStore((st) => st.reconnect);
  const copySettings = useStore((st) => st.copySettings);
  const pasteSettings = useStore((st) => st.pasteSettings);
  const setStyle = useStore((st) => st.setSpeakerStyle);
  const copied = useStore((st) => st.copied);
  const [name, setName] = useState(sp?.name ?? '');
  const [confirmDel, setConfirmDel] = useState(false);
  if (!s || !sp) return null;

  const saveName = () => {
    const n = name.trim();
    if (n && n !== sp.name) {
      void api.renameSpeaker(sp.id, n);
      useStore.setState((st) => (st.s ? { s: { ...st.s, speakers: st.s.speakers.map((x) => (x.id === sp.id ? { ...x, name: n } : x)) } } : {}));
    }
  };
  const nudge = (d: number) => setParam(sp.id, 'delayMs', Math.max(-500, Math.min(500, Math.round((sp.params.delayMs + d) * 10) / 10)));
  const roomColor = s.rooms.find((r) => r.id === sp.roomId)?.color ?? 'var(--c-ink3)';
  const shown = sp.color ?? roomColor;

  return (
    <Modal title={t("Réglages de l'enceinte")} onClose={close}>
      <div className="space-y-4">
        <Field label={t('Nom')}>
          <input className={inputCls} value={name} maxLength={60} onChange={(e) => setName(e.target.value)} onBlur={saveName} onKeyDown={(e) => e.key === 'Enter' && (e.target as HTMLInputElement).blur()} />
        </Field>

        <div className="grid grid-cols-3 gap-1.5">
          <button type="button" className={`${btnGhost} flex items-center justify-center gap-1.5 !px-2 !text-[11.5px]`} disabled={!sp.online && !sp.running}
            title={t("Joue un petit bip sur cette enceinte seulement, pour la reconnaître. Éteinte, elle s'allume le temps du bip.")} onClick={() => void identify(sp.id)}>
            <BellRing size={13} /> {t('Identifier')}
          </button>
          <button type="button" className={`${btnGhost} flex items-center justify-center gap-1.5 !px-2 !text-[11.5px]`} title={t('Copier égaliseur, effets, tonalité et retard')} onClick={() => copySettings(sp.id)}>
            <Copy size={13} /> {t('Copier')}
          </button>
          <button type="button" className={`${btnGhost} flex items-center justify-center gap-1.5 !px-2 !text-[11.5px]`} disabled={!copied || copied.from === sp.id}
            title={copied ? t('Coller les réglages de {name}', { name: copied.name }) : t('Copiez d’abord les réglages d’une autre enceinte')} onClick={() => pasteSettings([sp.id])}>
            <ClipboardPaste size={13} /> {t('Coller')}
          </button>
        </div>

        <Field label={t('Apparence')}>
          <div className="flex flex-wrap items-center gap-1.5">
            <button type="button" aria-pressed={!sp.color} onClick={() => setStyle(sp.id, { color: '' })} title={t('Couleur de la pièce')}
              className="h-7 rounded-[2px] border-2 px-2 text-[11px]" style={{ borderColor: !sp.color ? 'var(--c-ink)' : 'var(--c-border)', background: `${roomColor}22` }}>{t('Pièce')}</button>
            {ROOM_COLORS.map((c) => (
              <button key={c} type="button" aria-label={c} aria-pressed={sp.color === c} onClick={() => setStyle(sp.id, { color: c })}
                className="h-7 w-7 rounded-[2px] border-2" style={{ background: c, borderColor: sp.color === c ? 'var(--c-ink)' : 'transparent' }} />
            ))}
          </div>
          <div className="mt-2 grid grid-cols-7 gap-1.5">
            {Object.entries(SPEAKER_ICONS).map(([key, Icon]) => {
              const on = sp.icon === key;
              return (
                <button key={key} type="button" aria-label={key} aria-pressed={on} onClick={() => setStyle(sp.id, { icon: on ? '' : key })}
                  className="grid h-8 place-items-center rounded-[2px] border"
                  style={{ borderColor: on ? shown : 'var(--c-border)', background: on ? `${shown}22` : 'transparent', color: on ? shown : 'var(--c-ink3)' }}>
                  <Icon size={15} />
                </button>
              );
            })}
          </div>
        </Field>

        <Field label={t('Pièce')}>
          <div className="flex flex-wrap gap-1.5">
            {[...s.rooms, { id: '', name: t('Sans pièce'), color: 'var(--c-ink3)' }].map((r) => (
              <button key={r.id || 'none'} type="button" aria-pressed={sp.roomId === r.id} onClick={() => useStore.getState().moveSpeaker(sp.id, r.id)}
                className="rounded-[2px] border px-3 py-1 text-[11.5px] transition"
                style={{ borderColor: sp.roomId === r.id ? r.color : 'var(--c-line)', background: sp.roomId === r.id ? `${r.color}22` : 'transparent', color: sp.roomId === r.id ? r.color : 'var(--c-ink3)' }}>
                {'id' in r && r.id ? roomLabel(r) : r.name}
              </button>
            ))}
          </div>
        </Field>

        <Field label={t('Canal')}>
          <div className="grid grid-cols-4 gap-1">
            {MODES.map((m) => (
              <button key={m.id} type="button" aria-pressed={sp.params.mode === m.id} data-on={sp.params.mode === m.id} onClick={() => setParam(sp.id, 'mode', m.id)}
                className="key !h-auto !px-1 !py-1.5 !text-[9.5px] !tracking-[0.06em]">
                {t(m.label)}
              </button>
            ))}
          </div>
          <label className="mt-2 flex items-center justify-between text-[12px]">
            <span>{t('Inversion de phase (Ø)')}</span>
            <Toggle on={sp.params.phaseInvert} onChange={(v) => setParam(sp.id, 'phaseInvert', v)} label={t('Inversion de phase')} color="var(--c-ink)" />
          </label>
        </Field>

        <Field label={t('Synchronisation (retard)')}>
          <div className="flex items-center gap-4 rounded-[2px] border border-line bg-panel2 p-3">
            <RotaryKnob label="Sync" value={sp.params.delayMs} min={-500} max={500} defaultValue={0} step={1} fineStep={0.5} bipolar color="var(--c-signal)"
              format={(v) => `${fmtMs(v)}ms`} onChange={(v) => setParam(sp.id, 'delayMs', v)} size={56} />
            <div className="grid flex-1 grid-cols-4 gap-1">
              {[-10, -1, 1, 10].map((d) => (
                <button key={d} type="button" onClick={() => nudge(d)} className="rounded border border-line bg-inset py-1.5 font-mono text-[11px] text-mute transition hover:border-accent/50 hover:text-ink">
                  {d > 0 ? `+${d}` : d}
                </button>
              ))}
              <button type="button" onClick={() => setParam(sp.id, 'delayMs', 0)} className="col-span-4 rounded border border-line py-1 text-[11px] text-mute hover:text-ink">{t('Remettre à 0')}</button>
            </div>
          </div>
          <p className="mt-1.5 text-[10.5px] leading-snug text-dim">
            {t('Un retard négatif retarde les autres enceintes (on ne peut pas « avancer » un flux temps réel).')}
            {sp.protocol === 'sonos' && ` ${t('Une Sonos garde environ 0,3 s de tampon : le bouton Sync de la Console aligne les autres enceintes sur elle.')}`}
          </p>
        </Field>

        <Field label={t('Réseau')}>
          <div className="space-y-2 rounded-[2px] border border-line bg-panel2 p-3 text-[12px]">
            <div className="flex items-center gap-2">
              <NetBars sp={sp} />
              <span className="min-w-0 flex-1 leading-snug">{netText(sp)}</span>
            </div>
            <div className="flex flex-wrap gap-x-4 gap-y-1 text-[11.5px] text-mute">
              <span>{t('Latence estimée')} : <b className="num text-ink">{sp.latencyMs ? `${sp.latencyMs} ms` : '—'}</b></span>
              <span>{t('État')} : <b className="text-ink">{sp.streaming ? t('reçoit le flux') : sp.running ? t('démarrage…') : sp.online ? t('en ligne, éteinte') : t('hors ligne')}</b></span>
            </div>
            {sp.error && <div className="rounded border border-bad/50 bg-bad/10 px-2 py-1 text-[11.5px] text-bad">{sp.error}</div>}
            <div className="flex gap-1.5">
              <button type="button" className={`${btnGhost} flex flex-1 items-center justify-center gap-1.5 !py-1.5 !text-[11.5px]`} disabled={!sp.active} onClick={() => void reconnect(sp.id)}
                title={t('Arrête puis relance le flux de cette enceinte')}>
                <RefreshCw size={12} /> {t('Reconnecter')}
              </button>
              {!sp.online && (
                <button type="button" className={`${btnGhost} flex flex-1 items-center justify-center gap-1.5 !py-1.5 !text-[11.5px]`} onClick={() => void useStore.getState().scan()}>
                  <Search size={12} /> {t('Relancer la recherche')}
                </button>
              )}
            </div>
          </div>
        </Field>

        <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 rounded-[2px] bg-inset p-3 font-mono text-[10.5px] text-mute">
          <dt>{t('Protocole')}</dt><dd className="text-ink">{protocolBadge(sp)}</dd>
          <dt>{t('Adresse')}</dt><dd className="text-ink">{sp.address || '—'}{sp.port ? `:${sp.port}` : ''}</dd>
          {sp.model && (<><dt>{t('Modèle')}</dt><dd className="text-ink">{t(sp.model)}</dd></>)}
          {sp.group && (<><dt>{t('Groupe Sonos')}</dt><dd className="text-ink">{sp.group}</dd></>)}
          <dt>{t('Identifiant')}</dt><dd className="truncate text-ink" title={sp.id}>{sp.id}</dd>
        </dl>

        {confirmDel ? (
          <div className="flex gap-2">
            <button type="button" className="flex-1 rounded-[2px] bg-bad px-3 py-2 text-[12px] font-semibold text-white" onClick={async () => { apply(await api.removeSpeaker(sp.id)); close(); }}>{t('Oui, retirer')}</button>
            <button type="button" className={`${btnGhost} flex-1`} onClick={() => setConfirmDel(false)}>{t('Annuler')}</button>
          </div>
        ) : (
          <button type="button" className={`${btnGhost} flex w-full items-center justify-center gap-2 text-bad`} onClick={() => setConfirmDel(true)}>
            <Trash2 size={13} /> {t('Retirer cet appareil')}
          </button>
        )}
      </div>
    </Modal>
  );
}

export function RoomEditor({ id }: { id?: string }) {
  const close = useUi((u) => u.close);
  const room = useStore((st) => st.s?.rooms.find((r) => r.id === id));
  const [name, setName] = useState(room ? roomLabel(room) : '');
  const [icon, setIcon] = useState(room?.icon ?? 'Speaker');
  const [color, setColor] = useState(room?.color ?? ROOM_COLORS[0]);
  const [confirmDel, setConfirmDel] = useState(false);

  const save = async () => {
    if (!name.trim()) return;
    if (id) await api.updateRoom(id, { name: name.trim(), icon, color });
    else await api.createRoom(name.trim(), icon, color);
    useStore.getState().apply(await api.getState());
    close();
  };

  return (
    <Modal title={id ? t('Modifier la pièce') : t('Nouvelle pièce')} onClose={close}>
      <div className="space-y-4">
        <Field label={t('Nom')}><input autoFocus className={inputCls} value={name} maxLength={40} placeholder={t('Salon, Cuisine…')} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === 'Enter' && void save()} /></Field>
        <Field label={t('Icône')}>
          <div className="grid grid-cols-7 gap-1.5">
            {Object.entries(ROOM_ICONS).map(([key, Icon]) => (
              <button key={key} type="button" aria-label={key} aria-pressed={icon === key} onClick={() => setIcon(key)}
                className="grid h-9 place-items-center rounded-[2px] border transition"
                style={{ borderColor: icon === key ? color : 'var(--c-line)', background: icon === key ? `${color}22` : 'transparent', color: icon === key ? color : 'var(--c-ink3)' }}>
                <Icon size={16} />
              </button>
            ))}
          </div>
        </Field>
        <Field label={t('Couleur')}>
          <div className="flex gap-2">
            {ROOM_COLORS.map((c) => (
              <button key={c} type="button" aria-label={c} aria-pressed={color === c} onClick={() => setColor(c)} className="h-7 w-7 rounded-[2px] border-2 transition" style={{ background: c, borderColor: color === c ? 'var(--c-ink)' : 'transparent' }} />
            ))}
          </div>
        </Field>
        <div className="flex gap-2">
          <button type="button" className={`${btnPrimary} flex-1`} disabled={!name.trim()} onClick={() => void save()}>{id ? t('Enregistrer') : t('Créer')}</button>
          {id && (confirmDel ? (
            <button type="button" className="rounded-[2px] bg-bad px-3.5 py-2 text-[12px] font-semibold text-white" onClick={async () => { close(); await useStore.getState().deleteRooms([id]); }}>{t('Supprimer ?')}</button>
          ) : (
            <button type="button" aria-label={t('Supprimer la pièce')} className={`${btnGhost} text-bad`} onClick={() => setConfirmDel(true)}><Trash2 size={14} /></button>
          ))}
        </div>
      </div>
    </Modal>
  );
}

export function SettingsModal() {
  const close = useUi((u) => u.close);
  const s = useStore((st) => st.s);
  const apply = useStore((st) => st.apply);
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [updateMsg, setUpdateMsg] = useState<string | null>(null);
  const lang = useLang((l) => l.lang);
  const setZoom = useStore((st) => st.setZoom);
  const open = useUi((u) => u.open);

  useEffect(() => {
    api.getAutostart().then(setAutostart).catch(() => setAutostart(false));
  }, []);
  if (!s) return null;
  const set = s.settings;
  const update = async (patch: Partial<Settings>) => apply(await api.setSettings({ ...set, ...patch }));

  const Row = ({ title, hint, children }: { title: string; hint?: string; children: ReactNode }) => (
    <div className="flex items-center justify-between gap-4 py-2.5">
      <div className="min-w-0">
        <div className="text-[12.5px]">{title}</div>
        {hint && <div className="text-[10.5px] leading-snug text-dim">{hint}</div>}
      </div>
      {children}
    </div>
  );

  return (
    <Modal title={t('Réglages')} onClose={close} width={470}>
      <div className="divide-y divide-line">
        <Row title={t('Langue')} hint={t("Langue de l'interface et du menu de la zone de notification.")}>
          <div className="flex gap-1" role="group" aria-label={t('Langue')}>
            <button type="button" className="key !h-[26px]" data-on={lang === 'fr'} aria-pressed={lang === 'fr'} onClick={() => setLang('fr')}>Français</button>
            <button type="button" className="key !h-[26px]" data-on={lang === 'en'} aria-pressed={lang === 'en'} onClick={() => setLang('en')}>English</button>
          </div>
        </Row>
        <Row title={t("Taille de l'interface")} hint={t('Aussi avec Ctrl++, Ctrl+− et Ctrl+0.')}>
          <div className="flex shrink-0 items-center gap-1">
            <button type="button" className="key key-icon grid !h-[26px] !w-[26px] place-items-center" aria-label={t('Réduire')} disabled={s.uiZoom <= 0.7} onClick={() => void setZoom(s.uiZoom - ZOOM_STEP)}><Minus size={13} /></button>
            <button type="button" className="key num !h-[26px] w-[58px] !px-1 !text-[11.5px]" title={t('Revenir à 100 %')} onClick={() => void setZoom(1)}>{Math.round(s.uiZoom * 100)} %</button>
            <button type="button" className="key key-icon grid !h-[26px] !w-[26px] place-items-center" aria-label={t('Agrandir')} disabled={s.uiZoom >= 1.6} onClick={() => void setZoom(s.uiZoom + ZOOM_STEP)}><Plus size={13} /></button>
          </div>
        </Row>
        <Row title={t('Raccourcis clavier')} hint={t('Ctrl+Z pour annuler, Espace pour lecture/pause, M pour couper le son… Touche ? pour la liste.')}>
          <button type="button" className="key flex shrink-0 items-center gap-1.5 !h-[26px] !text-[11.5px]" onClick={() => open({ type: 'shortcuts' })}><Keyboard size={13} /> {t('Voir')}</button>
        </Row>
        <WidgetSettings />
        <Row title={t('Mises à jour')} hint={updateMsg ?? t('Au lancement puis toutes les 6 h, Rezo regarde sur GitHub si une nouvelle version est publiée. Le téléchargement est vérifié avec la somme SHA-256 de la version avant d\'être installé.')}>
          <div className="flex shrink-0 items-center gap-2">
            <button type="button" className="key !h-[26px] !text-[11.5px]" onClick={async () => { setUpdateMsg(t('Vérification…')); setUpdateMsg(await checkNow()); }}>{t('Vérifier')}</button>
            <Toggle on={set.checkUpdates} label={t('Vérifier les mises à jour')} onChange={(v) => void update({ checkUpdates: v })} />
          </div>
        </Row>
        <Row title={isWindows ? t('Démarrer avec Windows') : t("Ouvrir à l'ouverture de session")} hint={t('Se lance directement dans la zone de notification.')}>
          <Toggle on={!!autostart} label={isWindows ? t('Démarrer avec Windows') : t("Ouvrir à l'ouverture de session")} onChange={async (v) => { setAutostart(v); await api.setAutostart(v).catch(() => setAutostart(!v)); }} />
        </Row>
        {isWindows && <Row title="Smart Output" hint={t("Coupe la sortie du PC tant qu'une enceinte réseau joue, et rétablit exactement cette sortie ensuite (même si Windows a changé de sortie entre-temps).")}>
          <Toggle on={set.smartOutput} label="Smart Output" onChange={(v) => void update({ smartOutput: v })} />
        </Row>}
        {isWindows && <Row title={t('Période audio basse latence')} hint={t('Pendant les envois seulement : cale le moteur audio Windows à ~3 ms au lieu de 10 ms (enceintes natives). Change la période pour toutes les applications de cette sortie ; jamais appliqué sur une sortie Bluetooth. Désactivé par défaut.')}>
          <Toggle on={set.pinEnginePeriod} label={t('Période audio basse latence')} onChange={(v) => void update({ pinEnginePeriod: v })} />
        </Row>}
        <Row title={t('Latence Sonos minimale')} hint={t("Le flux Sonos démarre avec 50 ms d'avance au lieu de 200 ms : environ 120 ms de retard en moins (mesuré au micro : 300 ms au lieu de 420 ms). Désactivez si une Sonos en Wi-Fi coupe le son.")}>
          <Toggle on={set.sonosLowLatency} label={t('Latence Sonos minimale')} onChange={(v) => void update({ sonosLowLatency: v })} />
        </Row>
        <Row title={t('Trame Opus (UDP natif)')} hint={t('2,5 ms = latence minimale, 5 ms = plus robuste Wi-Fi.')}>
          <select className="rounded-[2px] border border-line bg-inset px-2 py-1.5 text-[12px]" value={set.udpFrameMs} onChange={(e) => void update({ udpFrameMs: Number(e.target.value) })}>
            <option value={2.5}>{t('2,5 ms')}</option>
            <option value={5}>5 ms</option>
          </select>
        </Row>
        <Row title={t('Débit Opus')}>
          <select className="rounded-[2px] border border-line bg-inset px-2 py-1.5 text-[12px]" value={set.bitrateKbps} onChange={(e) => void update({ bitrateKbps: Number(e.target.value) })}>
            {[96, 128, 160, 256].map((b) => <option key={b} value={b}>{b} kb/s</option>)}
          </select>
        </Row>
        <Row title={t('Redondance de trame (anti-perte)')} hint={t('Chaque paquet UDP embarque la trame précédente (×2 de débit).')}>
          <Toggle on={set.redundancy} label={t('Redondance')} onChange={(v) => void update({ redundancy: v })} />
        </Row>
        <Row title={t("Garder l'interface en mémoire")} hint={t('Réouverture instantanée, mais ~100 Mo de RAM en plus en arrière-plan.')}>
          <Toggle on={set.keepUiAlive} label={t("Garder l'interface en mémoire")} onChange={(v) => void update({ keepUiAlive: v })} />
        </Row>
        <Row title={t('Port du serveur HTTP')} hint={t('Actuellement {port}. Redémarrage requis.', { port: s.httpPort })}>
          <input className="w-24 select-text rounded-[2px] border border-line bg-inset px-2 py-1.5 text-right font-mono text-[12px]" inputMode="numeric" defaultValue={set.httpPort}
            onBlur={(e) => { const p = Number(e.target.value); if (p >= 1024 && p <= 65535 && p !== set.httpPort) void update({ httpPort: p }); }} />
        </Row>
      </div>
      <button type="button" className={`${btnGhost} mt-3 w-full text-bad`} onClick={() => void api.quit()}>{t('Quitter Rezo (arrête tous les flux)')}</button>
    </Modal>
  );
}
