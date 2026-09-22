import type { HomeMode, PathItem } from './pathItem';

export type SearchOptions = { fileName: boolean; path: boolean };

export function narrowPath(path: string, maxLength = 52): string {
  if (path.length <= maxLength) return path;
  const head = Math.ceil(maxLength * 0.36);
  const tail = maxLength - head - 1;
  return `${path.slice(0, head)}…${path.slice(-tail)}`;
}

export function filterItems(items: PathItem[], query: string, mode: HomeMode, options: SearchOptions): PathItem[] {
  const normalized = query.trim().toLocaleLowerCase();
  let visible = items.filter((item) => !item.excluded);

  if (mode === 'favorites') visible = visible.filter((item) => item.favorite);
  if (mode === 'recent') visible = [...visible].sort((a, b) => (b.lastUsedAt ?? '').localeCompare(a.lastUsedAt ?? ''));
  if (mode === 'frequent') visible = [...visible].sort((a, b) => b.useCount - a.useCount);
  if (!normalized) return visible;

  return visible.filter((item) => {
    const base = [item.name, ...item.tags].join(' ').toLocaleLowerCase();
    const extra = [options.fileName ? item.actualName : '', options.path ? item.path : ''].join(' ').toLocaleLowerCase();
    return `${base} ${extra}`.includes(normalized);
  });
}
