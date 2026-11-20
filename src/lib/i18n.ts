import { create } from 'zustand';
import { api } from './api';

export type Lang = 'fr' | 'en';

const KEY = 'rezo.lang';

function initialLang(): Lang {
  const q = typeof location !== 'undefined' ? new URLSearchParams(location.search).get('lang') : null;
  if (q === 'fr' || q === 'en') return q;
  try {
    const v = localStorage.getItem(KEY);
    if (v === 'fr' || v === 'en') return v;
  } catch {}
  return typeof navigator !== 'undefined' && navigator.language?.toLowerCase().startsWith('fr') ? 'fr' : 'en';
}

export const useLang = create<{ lang: Lang }>(() => ({ lang: initialLang() }));

export function setLang(lang: Lang) {
  try {
    localStorage.setItem(KEY, lang);
  } catch {}
  useLang.setState({ lang });
  document.documentElement.lang = lang;
  void api.setLanguage(lang).catch(() => {});
}

export function syncLangWithBackend() {
  const { lang } = useLang.getState();
  document.documentElement.lang = lang;
  void api.setLanguage(lang).catch(() => {});
  window.addEventListener('storage', (e) => {
    if (e.key === KEY && (e.newValue === 'fr' || e.newValue === 'en')) useLang.setState({ lang: e.newValue });
  });
}

export function t(fr: string, vars?: Record<string, string | number>): string {
  let s = useLang.getState().lang === 'en' ? (EN[fr] ?? fr) : fr;
  if (vars) for (const [k, v] of Object.entries(vars)) s = s.replaceAll(`{${k}}`, String(v));
  return s;
}

const EN: Record<string, string> = {
  'tout le son du PC': 'all PC audio',
  'Aucune enceinte active : Rezo ne capte rien et laisse les périphériques audio au repos': 'No active speaker: Rezo captures nothing and leaves audio devices idle',
  'Capture : {source}': 'Capture: {source}',
  'Capture active': 'Capturing',
  'Capture en erreur': 'Capture error',
  'En veille': 'Idle',
  '{n} reçoivent effectivement le flux': '{n} actually receiving the stream',
  '{n} enceinte en service': '{n} speaker active',
  '{n} enceintes en service': '{n} speakers active',
  "Smart Output : coupe la sortie du PC tant qu'une enceinte réseau est active": 'Smart Output: mutes the PC output while a network speaker is active',
  'PC coupé': 'PC muted',
  'PC audible': 'PC audible',
  'Démarrage du moteur audio…': 'Starting the audio engine…',

  'Sans pièce': 'No room',
  'Reçoit le flux': 'Receiving the stream',
  'Hors ligne': 'Offline',
  'En attente': 'Waiting',
  'Avancer de 1 ms': 'Advance by 1 ms',
  'Retarder de 1 ms': 'Delay by 1 ms',
  Envoi: 'Send',
  'Envoyer le son vers cette enceinte': 'Send audio to this speaker',
  'Inversion de phase': 'Phase invert',
  Pièce: 'Room',
  'Tout allumer / éteindre dans la pièce': 'Turn the whole room on / off',
  'Sortie, avec limiteur': 'Output, with limiter',
  'Haut-parleurs du PC : garder le son local en plus des enceintes': 'PC speakers: keep local audio in addition to the speakers',

  'Source du son': 'Audio source',
  'Tout le son du PC': 'All PC audio',
  'Applications qui jouent du son': 'Apps playing audio',
  'Recherche…': 'Searching…',
  'Aucune application ne produit de son.': 'No app is playing audio.',
  'Joue en ce moment': 'Playing now',
  "Mode application : le reste du son (Discord, notifications) n'est pas diffusé, et l'application reste audible sur le PC.":
    'App mode: other audio (Discord, notifications) is not streamed, and the app stays audible on the PC.',
  'Volume général': 'Master volume',
  'Mix « {name} » enregistré : enceintes actives et tous les réglages.': 'Mix “{name}” saved: active speakers and all settings.',
  Scènes: 'Scenes',
  'Confirmer la suppression de {name}': 'Confirm deleting {name}',
  'Supprimer {name}': 'Delete {name}',
  'Nom du mix': 'Mix name',
  'Enregistrer les enceintes actives et tous les réglages': 'Save the active speakers and all settings',
  'Enregistrer le mix': 'Save mix',
  Pièces: 'Rooms',
  Plan: 'Map',
  Console: 'Console',
  'Ajouter un appareil': 'Add a device',
  'Mixeur audio': 'Audio mixer',
  'Mixeur audio : sorties filaires / Bluetooth et volume par application': 'Audio mixer: wired / Bluetooth outputs and per-app volume',
  Réglages: 'Settings',
  'Désépingler la fenêtre': 'Unpin window',
  'Épingler la fenêtre': 'Pin window',
  'Épingler : la fenêtre ne se masque plus quand elle perd le focus': 'Pin: the window no longer hides when it loses focus',
  'Réduire dans la zone de notification': 'Minimize to the notification area',
  Vues: 'Views',

  'Bass, présence, loudness, largeur, compresseur, noise gate': 'Bass, presence, loudness, width, compressor, noise gate',
  Effets: 'Effects',
  'Effets de {name}': 'Effects of {name}',
  Fermer: 'Close',
  Tonalité: 'Tone',
  'Prés.': 'Pres.',
  'Larg.': 'Width',
  Compresseur: 'Compressor',
  'Rend le volume plus homogène : les passages forts sont rabaissés': 'Evens out the volume: loud passages are turned down',
  Seuil: 'Thresh.',
  'Gain+': 'Makeup',
  'Coupe le son sous le seuil (souffle, bruit de fond)': 'Mutes audio below the threshold (hiss, background noise)',

  'Génération du QR code…': 'Generating the QR code…',
  'Scannez avec un téléphone ou une tablette': 'Scan with a phone or tablet',
  'connecté au même Wi-Fi': 'on the same Wi-Fi',
  ': il devient une enceinte du PC, avec sa propre tranche de mixage.': ': it becomes a PC speaker with its own mixer channel.',
  'Interface réseau': 'Network interface',
  'Pare-feu Windows : autorisez Rezo sur les réseaux privés (voir le guide).': 'Windows Firewall: allow Rezo on private networks (see the guide).',
  Scanner: 'Scan',
  'Par adresse IP': 'By IP address',
  'Téléphone (QR)': 'Phone (QR)',
  'Recherche en cours…': 'Searching…',
  'Scanner Sonos / UPnP / mDNS': 'Scan for Sonos / UPnP / mDNS',
  'en ligne': 'online',
  'hors ligne': 'offline',
  'Aucun appareil trouvé. Vérifiez que le PC et les enceintes sont sur le même réseau (et que le pare-feu autorise Rezo).':
    'No device found. Check that the PC and the speakers are on the same network (and that the firewall allows Rezo).',
  "Type d'appareil": 'Device type',
  'Récepteur Rezo (UDP Opus)': 'Rezo receiver (UDP Opus)',
  'Adresse IP': 'IP address',
  Port: 'Port',
  'Connexion…': 'Connecting…',
  Ajouter: 'Add',
  Stéréo: 'Stereo',
  'Mono L+R': 'Mono L+R',
  'Gauche seul': 'Left only',
  'Droite seul': 'Right only',
  "Réglages de l'enceinte": 'Speaker settings',
  Nom: 'Name',
  Canal: 'Channel',
  'Inversion de phase (Ø)': 'Phase invert (Ø)',
  'Synchronisation (retard)': 'Sync (delay)',
  'Remettre à 0': 'Reset to 0',
  'Un retard négatif retarde les autres enceintes (on ne peut pas « avancer » un flux temps réel).':
    'A negative delay delays the other speakers instead (a live stream cannot be “advanced”).',
  'Les Sonos tamponnent le flux (≈ 1 à 3 s) : retardez les autres sorties pour les aligner.':
    'Sonos speakers buffer the stream (≈ 1–3 s): delay the other outputs to line them up.',
  Protocole: 'Protocol',
  Adresse: 'Address',
  Modèle: 'Model',
  'Groupe Sonos': 'Sonos group',
  Identifiant: 'ID',
  'Oui, retirer': 'Yes, remove',
  Annuler: 'Cancel',
  'Retirer cet appareil': 'Remove this device',
  'Modifier la pièce': 'Edit room',
  'Nouvelle pièce': 'New room',
  'Salon, Cuisine…': 'Living room, Kitchen…',
  Icône: 'Icon',
  Couleur: 'Color',
  Enregistrer: 'Save',
  Créer: 'Create',
  'Supprimer ?': 'Delete?',
  'Supprimer la pièce': 'Delete room',

  Langue: 'Language',
  "Langue de l'interface et du menu de la zone de notification.": 'Language of the interface and the notification-area menu.',
  'Démarrer avec Windows': 'Start with Windows',
  'Se lance directement dans la zone de notification.': 'Starts straight into the notification area.',
  "Coupe la sortie du PC tant qu'une enceinte réseau joue, et rétablit exactement cette sortie ensuite (même si Windows a changé de sortie entre-temps).":
    'Mutes the PC output while a network speaker plays, then restores exactly that output (even if Windows switched outputs in the meantime).',
  'Période audio basse latence': 'Low-latency audio period',
  'Pendant les envois seulement : cale le moteur audio Windows à ~3 ms au lieu de 10 ms (enceintes natives). Change la période pour toutes les applications de cette sortie ; jamais appliqué sur une sortie Bluetooth. Désactivé par défaut.':
    'While streaming only: sets the Windows audio engine to ~3 ms instead of 10 ms (native receivers). Changes the period for every app on that output; never applied to Bluetooth outputs. Off by default.',
  'Trame Opus (UDP natif)': 'Opus frame (native UDP)',
  '2,5 ms = latence minimale, 5 ms = plus robuste Wi-Fi.': '2.5 ms = lowest latency, 5 ms = more robust on Wi-Fi.',
  '2,5 ms': '2.5 ms',
  'Débit Opus': 'Opus bitrate',
  'Redondance de trame (anti-perte)': 'Frame redundancy (loss protection)',
  'Chaque paquet UDP embarque la trame précédente (×2 de débit).': 'Each UDP packet also carries the previous frame (2× bitrate).',
  Redondance: 'Redundancy',
  "Garder l'interface en mémoire": 'Keep the interface in memory',
  'Réouverture instantanée, mais ~100 Mo de RAM en plus en arrière-plan.': 'Instant reopening, but ~100 MB more RAM in the background.',
  'Port du serveur HTTP': 'HTTP server port',
  'Actuellement {port}. Redémarrage requis.': 'Currently {port}. Restart required.',
  'Quitter Rezo (arrête tous les flux)': 'Quit Rezo (stops all streams)',

  'groupe {name}': 'group {name}',
  'Envoyer vers {name}': 'Send to {name}',
  'Volume {name}': 'Volume {name}',
  'Toute la pièce {name}': 'Whole room {name}',
  'Volume de la pièce {name}': 'Room volume {name}',
  'Aucune enceinte trouvée': 'No speaker found',
  "Rezo cherche les Sonos, les enceintes UPnP/DLNA et les récepteurs Rezo du réseau. Un téléphone peut aussi servir d'enceinte avec un QR code.":
    'Rezo looks for Sonos, UPnP/DLNA speakers and Rezo receivers on the network. A phone can also become a speaker with a QR code.',
  'Relancer la recherche': 'Search again',
  "Placez-les dans une pièce depuis l'onglet Plan ou leurs réglages.": 'Put them in a room from the Map tab or their settings.',

  'Glisser vers une autre pièce · cliquer pour régler': 'Drag to another room · click for settings',
  'Allumer ou éteindre {name}': 'Turn {name} on or off',
  Allumée: 'On',
  Éteinte: 'Off',
  'Déposez une enceinte ici': 'Drop a speaker here',
  '{n} / {m} active': '{n} / {m} active',
  '{n} / {m} actives': '{n} / {m} active',

  'Web MIDI indisponible dans cette fenêtre.': 'Web MIDI is unavailable in this window.',
  'Accès MIDI refusé ou indisponible': 'MIDI access denied or unavailable',
  'Cliquez un contrôle, puis bougez un potard, fader ou bouton du contrôleur. Clic droit : délier.':
    'Click a control, then move a knob, fader or button on your controller. Right-click: unbind.',
  'MIDI inactif : les contrôleurs restent libres pour vos autres logiciels (MIDI Learn pour lier).':
    'MIDI off: controllers stay free for your other software (use MIDI Learn to bind).',
  'Contrôleur : {list}': 'Controller: {list}',
  'Aucun contrôleur MIDI détecté. Branchez-en un en USB.': 'No MIDI controller detected. Plug one in over USB.',
  '{n} assignation': '{n} binding',
  '{n} assignations': '{n} bindings',
  'Tout effacer': 'Clear all',
  'Aucune enceinte. Ajoutez-en depuis la barre du haut.': 'No speakers. Add some from the top bar.',
  'Pièce {name}': 'Room {name}',

  'Filaire (jack / intégré)': 'Wired (jack / built-in)',
  'HDMI / DisplayPort / optique': 'HDMI / DisplayPort / optical',
  Réseau: 'Network',
  'Autre sortie': 'Other output',
  'Réactiver le son de {name}': 'Unmute {name}',
  'Couper le son de {name}': 'Mute {name}',
  'Son coupé : cliquer pour réactiver': 'Muted: click to unmute',
  'Couper le son': 'Mute',
  'Volume de {name}': 'Volume of {name}',
  "Sorties du PC (filaires, Bluetooth, HDMI…) et volume de chaque application. Les réglages sont ceux de Windows : ils s'appliquent aussitôt.":
    'PC outputs (wired, Bluetooth, HDMI…) and per-app volume. These are Windows settings: changes apply immediately.',
  'Lecture des périphériques audio…': 'Reading audio devices…',
  'Impossible de lire les périphériques audio.': 'Could not read the audio devices.',
  'Sorties audio': 'Audio outputs',
  'Aucune sortie audio active. Branchez un casque ou connectez un appareil Bluetooth.': 'No active audio output. Plug in headphones or connect a Bluetooth device.',
  'Sortie par défaut': 'Default output',
  "Smart Output est actif (haut-parleurs du PC coupés) : désactivez-le ou arrêtez les envois d'abord.": 'Smart Output is active (PC speakers muted): turn it off or stop streaming first.',
  'Faire de cette sortie la sortie par défaut de Windows': 'Make this the Windows default output',
  'Utiliser cette sortie': 'Use this output',
  'Smart Output est actif : les haut-parleurs du PC sont coupés pendant les envois, le changement de sortie par défaut est donc bloqué.':
    'Smart Output is active: the PC speakers are muted while streaming, so changing the default output is blocked.',
  Applications: 'Apps',
  "Aucune application n'utilise le son pour l'instant.": 'No app is using audio right now.',
  'Sur : {name}': 'On: {name}',
  'Pour envoyer une application précise vers une autre sortie, utilisez Paramètres Windows › Système › Son › Mélangeur de volume : Windows ne laisse pas une application tierce le faire de façon fiable.':
    'To send a specific app to another output, use Windows Settings › System › Sound › Volume mixer: Windows does not let third-party apps do this reliably.',
  'Sons système Windows': 'Windows system sounds',
  'Processus {n}': 'Process {n}',
};
