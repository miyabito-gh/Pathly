import type { HomeMode, PathItem } from './pathItem';

export type SearchOptions = { fileName: boolean; path: boolean };
export type SortMode = 'frequency' | 'name' | 'last-used';

type QueryParts = { text: string; tags: string[]; categories: string[] };

/** Extracts #tag and @category tokens; all other input remains ordinary search text. */
export function parseSearchQuery(query: string): QueryParts {
  const text: string[] = [];
  const tags: string[] = [];
  const categories: string[] = [];
  const tokens = query.match(/(?:[#@]"[^"\n]*"|\S+)/g) ?? [];

  for (const token of tokens) {
    const prefix = token[0];
    const target = prefix === '#' ? tags : prefix === '@' ? categories : undefined;
    if (!target) {
      text.push(token);
      continue;
    }

    const quoted = token.length >= 3 && token[1] === '"' && token.endsWith('"');
    const value = quoted ? token.slice(2, -1) : token.slice(1);
    if (!value.trim()) text.push(token);
    else target.push(value.toLocaleLowerCase());
  }

  return { text: text.join(' ').trim().toLocaleLowerCase(), tags, categories };
}

export function narrowPath(path: string, maxLength = 52): string {
  if (path.length <= maxLength) return path;
  const head = Math.ceil(maxLength * 0.36);
  const tail = maxLength - head - 1;
  return `${path.slice(0, head)}…${path.slice(-tail)}`;
}

export function filterItems(items: PathItem[], query: string, mode: HomeMode, options: SearchOptions): PathItem[] {
  const parsed = parseSearchQuery(query);
  let visible = items.filter((item) => !item.excluded);

  if (mode === 'favorites') visible = visible.filter((item) => item.favorite);
  if (mode === 'recent') {
    visible = visible
      .filter((item) => item.lastUsedAt != null)
      .sort((a, b) => b.lastUsedAt!.localeCompare(a.lastUsedAt!));
  }
  if (mode === 'frequent') {
    visible = visible
      .filter((item) => item.useCount > 0)
      .sort((a, b) => b.useCount - a.useCount);
  }
  if (!parsed.text && !parsed.tags.length && !parsed.categories.length) return visible;

  return visible.filter((item) => {
    const base = [item.name, ...item.tags, item.category ?? ''].join(' ').toLocaleLowerCase();
    const extra = [options.fileName ? item.actualName : '', options.path ? item.path : ''].join(' ').toLocaleLowerCase();
    return (!parsed.text || `${base} ${extra}`.includes(parsed.text))
      && parsed.tags.every((term) => item.tags.some((tag) => tag.toLocaleLowerCase().includes(term)))
      && parsed.categories.every((term) => (item.category ?? '').toLocaleLowerCase().includes(term));
  });
}

export function sortItems(items: PathItem[], sortMode: SortMode): PathItem[] {
  return [...items].sort((a, b) => {
    if (sortMode === 'name') return a.name.localeCompare(b.name, 'ja');
    if (sortMode === 'last-used') {
      const aDate = a.rawLastUsedAt ?? a.lastUsedAt ?? '';
      const bDate = b.rawLastUsedAt ?? b.lastUsedAt ?? '';
      return bDate.localeCompare(aDate);
    }
    return b.useCount - a.useCount;
  });
}
