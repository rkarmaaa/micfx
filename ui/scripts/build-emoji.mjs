// Genera ui/src/lib/emoji/data.json da emojibase-data: tutte le emoji, con nomi e parole chiave
// in italiano e inglese (più gli shortcode stile GitHub) per la ricerca.
//
//   node ui/scripts/build-emoji.mjs
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const base = join(root, 'node_modules', 'emojibase-data');
const read = (p) => JSON.parse(readFileSync(join(base, p), 'utf8'));

const it = read('it/data.json');
const en = new Map(read('en/data.json').map((e) => [e.hexcode, e]));
const github = read('en/shortcodes/github.json');
const cldrIt = read('it/shortcodes/cldr.json');
const messages = read('it/messages.json');

const norm = (s) =>
  s
    .toLowerCase()
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .replace(/[_:]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();

const asList = (v) => (v == null ? [] : Array.isArray(v) ? v : [v]);

const groups = messages.groups
  .filter((g) => g.key !== 'component')
  .map((g) => ({ id: g.order, key: g.key, label: g.message.charAt(0).toUpperCase() + g.message.slice(1) }));

const out = [];
for (const e of it) {
  if (e.group === undefined || e.group === 2) continue; // componenti (toni della pelle, capelli)
  const eng = en.get(e.hexcode);
  const words = new Set();
  for (const w of [e.label, ...(e.tags ?? []), eng?.label, ...(eng?.tags ?? []), ...asList(github[e.hexcode]), ...asList(cldrIt[e.hexcode])]) {
    if (w) words.add(norm(w));
  }
  const label = e.label.charAt(0).toUpperCase() + e.label.slice(1);
  const row = {
    e: e.emoji,
    n: label,
    k: [...words].join('|'),
    l: eng ? norm(eng.label) : '',
    g: e.group,
    v: e.version,
    o: e.order,
  };
  if (e.skins?.length) {
    // Solo le varianti a tono singolo (1–5), nello stesso ordine del selettore.
    const byTone = new Map();
    for (const s of e.skins) {
      if (typeof s.tone === 'number' && !byTone.has(s.tone)) byTone.set(s.tone, s.emoji);
    }
    if (byTone.size === 5) row.s = [1, 2, 3, 4, 5].map((t) => byTone.get(t));
  }
  out.push(row);
}
out.sort((a, b) => a.o - b.o);
for (const r of out) delete r.o;

const dest = join(root, 'ui', 'src', 'lib', 'emoji', 'data.json');
mkdirSync(dirname(dest), { recursive: true });
writeFileSync(dest, JSON.stringify({ groups, emojis: out }));
console.log(`emoji: ${out.length} voci, ${groups.length} categorie → ${dest}`);
