// Six-band equaliser: bands, gain range and presets (gains in dB for 60 Hz, 150 Hz, 400 Hz, 1 kHz, 2.4 kHz, 15 kHz).

export const EQ_BANDS = [
  { hz: 60, label: '60Hz' },
  { hz: 150, label: '150Hz' },
  { hz: 400, label: '400Hz' },
  { hz: 1000, label: '1KHz' },
  { hz: 2400, label: '2.4KHz' },
  { hz: 15000, label: '15KHz' },
] as const;

export const EQ_MAX_DB = 12;
export const EQ_STEP_DB = 0.1;

export type EqGains = [number, number, number, number, number, number];

export interface EqPreset {
  id: string;
  // French label, translated with t()
  label: string;
  gains: EqGains;
}

export const FLAT: EqGains = [0, 0, 0, 0, 0, 0];
export const CUSTOM_ID = 'custom';

export const EQ_PRESETS: EqPreset[] = [
  { id: 'flat', label: 'Aucune correction', gains: FLAT },
  { id: 'acoustic', label: 'Acoustique', gains: [4.0, 2.5, 1.0, 1.5, 3.0, 4.0] },
  { id: 'bass-boost', label: 'Amplificateur de basse', gains: [6.5, 4.5, 1.0, 0.0, 0.0, 0.0] },
  { id: 'bass-cut', label: 'Réducteur de basse', gains: [-6.0, -4.0, -1.0, 0.0, 0.0, 0.0] },
  { id: 'classical', label: 'Classique', gains: [4.5, 3.0, -1.5, 0.0, 2.5, 4.0] },
  { id: 'dance', label: 'Dance', gains: [5.0, 3.5, 0.5, 0.0, 2.5, 4.5] },
  { id: 'deep', label: 'Profond', gains: [3.0, 1.5, 2.5, 2.0, 1.0, -1.5] },
  { id: 'electronic', label: 'Électro', gains: [5.5, 3.0, -1.0, 1.5, 3.5, 5.0] },
  { id: 'hip-hop', label: 'Hip-hop', gains: [5.5, 4.0, 0.5, -1.0, 2.0, 3.0] },
  { id: 'jazz', label: 'Jazz', gains: [3.5, 2.0, -1.5, 1.5, 3.0, 4.0] },
  { id: 'latin', label: 'Latino', gains: [4.0, 2.0, -1.0, 0.0, 3.0, 4.5] },
  { id: 'loudness', label: 'Niveau sonore', gains: [5.0, 2.5, -1.5, -1.0, 2.5, 5.0] },
  { id: 'lounge', label: 'Lounge', gains: [-2.5, -1.0, 2.0, 3.0, 1.0, -1.5] },
  { id: 'piano', label: 'Piano', gains: [2.5, 1.5, 0.0, 2.5, 3.5, 3.0] },
  { id: 'pop', label: 'Pop', gains: [-1.5, 1.5, 3.5, 3.0, 1.0, -1.5] },
  { id: 'rnb', label: 'RnB', gains: [5.0, 3.5, -1.5, 1.0, 2.5, 3.5] },
  { id: 'rock', label: 'Rock', gains: [4.5, 2.5, -2.0, 1.0, 3.5, 4.5] },
  { id: 'small-speakers', label: 'Petites enceintes', gains: [5.5, 3.5, 1.0, 0.0, -1.5, -2.5] },
  { id: 'spoken-word', label: 'Texte parlé', gains: [-3.5, -1.5, 2.5, 4.5, 3.5, -1.0] },
  { id: 'treble-boost', label: "Amplificateur d'aigu", gains: [0.0, 0.0, 0.0, 1.5, 4.0, 6.5] },
];

export function clampGain(db: number): number {
  const v = Math.round(Math.min(EQ_MAX_DB, Math.max(-EQ_MAX_DB, db)) / EQ_STEP_DB) * EQ_STEP_DB;
  return Math.round(v * 10) / 10 || 0;
}

export function presetById(id: string): EqPreset | undefined {
  return EQ_PRESETS.find((p) => p.id === id);
}

export function formatGain(db: number): string {
  return `${db > 0 ? '+' : ''}${db.toFixed(1)} dB`;
}
