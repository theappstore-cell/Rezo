import type { CSSProperties } from 'react';

// Accent colour of a `.key` button or pad when it is on. Coloured accents get white text; the default (ink) accent
// uses the theme's text-on-ink colour, so an "on" button stays readable in the dark theme.
export function tint(color: string): CSSProperties {
  return { ['--c' as string]: color, ['--c-fg' as string]: '#fff' };
}
