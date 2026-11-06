// Testi per l'utente a partire dai codici stabili del core (SPEC §15.1: "i testi li decide la UI").
import type { ErrorCode } from '../api';

export const STATUS_TEXT: Partial<Record<ErrorCode, { title: string; body: string }>> = {
  cable_missing: {
    title: 'VB-Cable non trovato',
    body: 'Senza VB-Cable i tuoi amici non sentono i suoni: li senti solo tu. Installalo una volta e riavvia il PC.',
  },
  mic_not_routed: {
    title: 'La tua voce non arriva a Discord',
    body: 'Attiva "Ascolta questo dispositivo" sul tuo microfono verso CABLE Input, così su Discord arrivano voce e suoni insieme.',
  },
  default_is_cable: {
    title: 'CABLE Input è il dispositivo predefinito',
    body: 'Così non senti più l\'audio del PC. Imposta le cuffie come dispositivo di riproduzione predefinito.',
  },
  discord_filters_sounds: {
    title: 'Discord sta togliendo i suoni',
    body: 'La soppressione del rumore (Krisp) li tratta come rumore. In Discord → Voce e video → Soppressione del rumore: Nessuna.',
  },
  discord_wrong_input: {
    title: 'Discord non ascolta il microfono virtuale',
    body: 'In Discord → Voce e video → Dispositivo di input: CABLE Output (VB-Audio Virtual Cable).',
  },
  discord_push_to_talk: {
    title: 'Discord è in push-to-talk',
    body: 'I suoni passano solo mentre tieni premuto il tasto. Attiva il relay push-to-talk nelle impostazioni di MicFX.',
  },
  rate_mismatch: {
    title: 'Frequenza diversa da 48 kHz',
    body: 'Funziona lo stesso, ma per la qualità migliore imposta 48000 Hz su cuffie e VB-Cable.',
  },
  cable_busy: {
    title: 'VB-Cable occupato',
    body: 'Un\'altra app usa CABLE Input in modo esclusivo. Disattiva "Consenti alle applicazioni di assumere il controllo esclusivo".',
  },
};

export const ERROR_TEXT: Partial<Record<ErrorCode, string>> = {
  hotkey_conflict: 'Combinazione già usata da un\'altra app.',
  hotkey_invalid: 'Combinazione non valida.',
  import_unsupported: 'Formato non supportato.',
  import_decode_failed: 'File danneggiato o non leggibile.',
  import_too_long: 'File troppo lungo.',
  import_duplicate: 'Già nella libreria.',
  import_empty: 'Il file non contiene audio.',
  import_not_found: 'File non trovato.',
  disk_full: 'Spazio su disco esaurito.',
  sound_not_found: 'Suono non trovato.',
  outputs_disabled: 'Microfono e cuffie sono disattivati.',
  device_lost: 'Dispositivo audio non disponibile.',
  ptt_send_failed: 'Push-to-talk bloccato da Windows.',
  internal: 'Errore interno.',
};

export function errorText(code: ErrorCode, fallback: string): string {
  return fallback || ERROR_TEXT[code] || 'Si è verificato un errore.';
}
