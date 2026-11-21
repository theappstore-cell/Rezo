import {
  BedDouble, Cast, CookingPot, Monitor, Radio, Smartphone, Sofa, Speaker, TreePalm, Wifi,
  Music2, Bath, Dumbbell, Gamepad2, Car, Baby, Warehouse, Armchair,
} from 'lucide-react';
import type { LucideIcon } from 'lucide-react';
import type { Protocol, Speaker as Spk } from './types';
import { t } from './i18n';

export const ROOM_ICONS: Record<string, LucideIcon> = {
  Sofa, CookingPot, Monitor, BedDouble, TreePalm, Speaker, Music2, Bath, Dumbbell, Gamepad2, Car, Baby, Warehouse, Armchair,
};

export function roomIcon(name: string): LucideIcon {
  return ROOM_ICONS[name] ?? Speaker;
}

export function protocolIcon(p: Protocol): LucideIcon {
  switch (p) {
    case 'sonos': return Speaker;
    case 'upnp': return Cast;
    case 'native': return Radio;
    case 'web': return Smartphone;
    default: return Wifi;
  }
}

export function protocolBadge(sp: Spk): string {
  switch (sp.protocol) {
    case 'sonos':
      if (sp.syncedTo) return t('Sonos groupée');
      if (sp.useLineIn && sp.lineIn) return t('Sonos entrée ligne');
      return sp.latencyMs ? `Sonos ~${sp.latencyMs}ms` : 'Sonos UPnP';
    case 'upnp': return 'UPnP/DLNA';
    case 'native': return sp.latencyMs ? `UDP ${sp.latencyMs}ms` : 'UDP';
    case 'web': return sp.latencyMs ? `Web ${sp.latencyMs}ms` : 'Web';
  }
}

export const ROOM_COLORS = ['#d9480f', '#5c940d', '#1c7ed6', '#9c36b5', '#c2255c', '#b08900', '#0b7285', '#6b5b4b'];
