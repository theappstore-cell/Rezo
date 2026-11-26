import { t } from '../lib/i18n';
import type { Speaker } from '../lib/types';

// 0 = no answer to the ping, 1 = poor … 4 = excellent, from the round trip and the share of lost echoes.
export function netLevel(sp: Speaker): number {
  const n = sp.net;
  if (!n) return 0;
  if (n.lossPct >= 15 || n.rttMs >= 150) return 1;
  if (n.lossPct >= 5 || n.rttMs >= 60 || n.jitterMs >= 40) return 2;
  if (n.lossPct >= 1 || n.rttMs >= 20 || n.jitterMs >= 15) return 3;
  return 4;
}

const LEVEL_COLOR = ['var(--c-border-strong)', 'var(--c-bad)', 'var(--c-warn)', 'var(--c-ok)', 'var(--c-ok)'];

export function netText(sp: Speaker): string {
  const n = sp.net;
  if (!sp.online && !sp.running) return t('Hors ligne');
  if (!n) return t("Ne répond pas au ping : son pare-feu le bloque peut-être (cela n'empêche pas le son de passer).");
  const quality = [t('—'), t('Réseau faible'), t('Réseau moyen'), t('Bon réseau'), t('Excellent réseau')][netLevel(sp)];
  return t('{quality} : ping {rtt} ms, gigue {jitter} ms, pertes {loss} % (sur {n} essais)', {
    quality,
    rtt: n.rttMs,
    jitter: n.jitterMs,
    loss: n.lossPct,
    n: n.samples,
  });
}

// Signal-strength style bars; the tooltip gives the numbers.
export function NetBars({ sp, className = '' }: { sp: Speaker; className?: string }) {
  const level = netLevel(sp);
  const color = LEVEL_COLOR[level];
  return (
    <span className={`inline-flex h-[11px] shrink-0 items-end gap-[1.5px] ${className}`} title={netText(sp)} role="img" aria-label={netText(sp)}>
      {[0, 1, 2, 3].map((i) => (
        <span
          key={i}
          className="w-[3px] rounded-[1px]"
          style={{ height: 4 + i * 2.3, background: i < level ? color : 'transparent', border: i < level ? 'none' : '1px solid var(--c-border-strong)' }}
        />
      ))}
    </span>
  );
}
