// Voci delle select dei dispositivi: le stesse in Impostazioni → Audio e nella barra dei volumi.
import type { Device, Mic } from '../api';
import { CABLE_NAME, OS_NAME } from './platform';

type Option = { value: string; label: string };

/** Cuffie: il predefinito del sistema (`''`) o un dispositivo che non sia il cavo. */
export function monitorOptions(devices: Device[]): Option[] {
  return [
    { value: '', label: `Predefinito di ${OS_NAME}` },
    ...devices.filter((d) => !d.is_cable).map((d) => ({ value: d.id, label: d.name + (d.is_default ? ' (predefinito)' : '') })),
  ];
}

/** Microfono virtuale: rilevato da solo (`''`) o scelto a mano. */
export function cableOptions(devices: Device[]): Option[] {
  return [{ value: '', label: `Rileva ${CABLE_NAME} automaticamente` }, ...devices.map((d) => ({ value: d.id, label: d.name }))];
}

/** La voce che MicFX manda nel cavo: il microfono predefinito (`''`) o uno scelto. */
export function micOptions(mics: Mic[]): Option[] {
  return [
    { value: '', label: `Microfono predefinito di ${OS_NAME}` },
    ...mics.map((m) => ({ value: m.id, label: m.name + (m.is_default ? ' (predefinito)' : '') })),
  ];
}
