import { useEffect, useState } from 'react';
import { Check, Loader2, Network, QrCode, Search, ShieldCheck, Wifi } from 'lucide-react';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import { t } from '../lib/i18n';
import { useUi } from '../lib/ui';

function Step({ icon: Icon, title, children, done }: { icon: typeof Wifi; title: string; children: React.ReactNode; done?: boolean }) {
  return (
    <li className="flex gap-3 rounded-md border border-line bg-panel p-3">
      <span className={`grid h-7 w-7 shrink-0 place-items-center rounded-full ${done ? 'bg-ok text-white' : 'bg-inset text-mute'}`}>
        {done ? <Check size={15} /> : <Icon size={15} />}
      </span>
      <div className="min-w-0 flex-1">
        <div className="text-[13px] font-semibold">{title}</div>
        <div className="mt-0.5 text-[12px] leading-snug text-mute">{children}</div>
      </div>
    </li>
  );
}

// Shown while Rezo knows no speaker: what to check, and the three ways to add one.
export function Welcome() {
  const scanning = useStore((st) => st.scanning);
  const scan = useStore((st) => st.scan);
  const open = useUi((u) => u.open);
  const [ips, setIps] = useState<string[] | null>(null);

  useEffect(() => {
    api.getReceiverInfo().then((i) => setIps(i.ips)).catch(() => setIps([]));
  }, []);

  return (
    <div className="mx-auto flex w-full max-w-[520px] flex-col gap-4 py-6">
      <div className="text-center">
        <h2 className="text-[18px] font-semibold">{t('Bienvenue dans Rezo')}</h2>
        <p className="mt-1 text-[13px] leading-snug text-mute">
          {t('Rezo envoie le son de ce PC vers les enceintes de la maison : Sonos, enceintes UPnP/DLNA, autres PC avec le récepteur Rezo, et téléphones.')}
        </p>
      </div>

      <div className="flex items-center justify-center gap-2 text-[12.5px] text-mute" role="status">
        {scanning ? <Loader2 size={14} className="animate-spin" /> : <Search size={14} />}
        {scanning ? t('Recherche des enceintes sur le réseau…') : t('Aucune enceinte trouvée pour le moment.')}
      </div>

      <ol className="flex flex-col gap-2">
        <Step icon={Wifi} title={t('Même réseau')} done={!!ips && ips.length > 0}>
          {t('Le PC et les enceintes doivent être sur le même réseau (même box, même Wi-Fi, pas le réseau invité).')}
          {ips && ips.length > 0 && <> {t('Adresse de ce PC :')} <code className="select-text rounded bg-inset px-1 font-mono text-[11.5px] text-ink">{ips.join(', ')}</code></>}
          {ips && ips.length === 0 && <span className="text-bad"> {t('Aucune adresse réseau locale détectée.')}</span>}
        </Step>
        <Step icon={ShieldCheck} title={t('Pare-feu')}>
          {t('Au premier lancement, Windows demande d’autoriser Rezo : cochez « Réseaux privés ». Si vous avez refusé, autorisez Rezo dans Sécurité Windows > Pare-feu > Autoriser une application.')}
        </Step>
        <Step icon={Network} title={t('Enceintes allumées')}>
          {t('Les Sonos doivent être configurées dans l’app Sonos. Pour un autre PC, lancez le récepteur Rezo dessus.')}
        </Step>
      </ol>

      <div className="grid grid-cols-1 gap-2 min-[420px]:grid-cols-3">
        <button type="button" onClick={() => void scan()} disabled={scanning} className="key key-primary flex items-center justify-center gap-1.5 !h-9">
          <Search size={14} /> {scanning ? t('Recherche…') : t('Relancer la recherche')}
        </button>
        <button type="button" onClick={() => open({ type: 'add' })} className="key flex items-center justify-center gap-1.5 !h-9">
          <Network size={14} /> {t('Ajouter par adresse IP')}
        </button>
        <button type="button" onClick={() => open({ type: 'add' })} className="key flex items-center justify-center gap-1.5 !h-9">
          <QrCode size={14} /> {t('Utiliser un téléphone')}
        </button>
      </div>
      <p className="text-center text-[11.5px] text-dim">{t('Une fois les enceintes trouvées : clic droit > Identifier pour savoir laquelle est laquelle, et touche ? pour les raccourcis.')}</p>
    </div>
  );
}
