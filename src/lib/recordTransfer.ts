import type { ItemKind, PathItem } from './pathItem';

export const recordTransferHeaders = [
  'id',
  'name',
  'actual_name',
  'kind',
  'path',
  'tags',
  'category',
  'memo',
  'favorite',
  'excluded',
  'use_count',
  'last_used_at'
] as const;

export type RecordWrite = {
  id?: number;
  name: string;
  path: string;
  kindHint?: ItemKind;
  tags: string[];
  category?: string | null;
  memo: string;
  favorite: boolean;
  useCount: number;
  lastUsedAt?: string | null;
  excluded: boolean;
};

export type ParsedRecordRow = {
  rowNumber: number;
  actualName: string;
  record?: RecordWrite;
  error?: string;
};

function protectSpreadsheetFormula(value: string): string {
  return /^[=+\-@]/.test(value) ? `'${value}` : value;
}

function restoreProtectedFormula(value: string): string {
  return /^'[=+\-@]/.test(value) ? value.slice(1) : value;
}

function quoteCell(value: string, delimiter: ',' | '\t'): string {
  const safe = protectSpreadsheetFormula(value);
  if (safe.includes(delimiter) || /["\r\n]/.test(safe)) return `"${safe.replaceAll('"', '""')}"`;
  return safe;
}

function itemCells(item: PathItem): string[] {
  return [
    String(item.id),
    item.name,
    item.actualName,
    item.kind,
    item.path,
    item.tags.join('|'),
    item.category ?? '',
    item.memo,
    String(item.favorite),
    String(item.excluded),
    String(item.useCount),
    item.rawLastUsedAt ?? item.lastUsedAt ?? ''
  ];
}

export function serializeRecords(items: PathItem[], delimiter: ',' | '\t'): string {
  return [recordTransferHeaders.join(delimiter), ...items.map((item) => itemCells(item).map((cell) => quoteCell(cell, delimiter)).join(delimiter))].join('\r\n');
}

export function parseDelimited(text: string, delimiter: ',' | '\t'): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let cell = '';
  let quoted = false;
  const source = text.replace(/^\uFEFF/, '');

  for (let index = 0; index < source.length; index += 1) {
    const character = source[index];
    if (quoted) {
      if (character === '"' && source[index + 1] === '"') {
        cell += '"';
        index += 1;
      } else if (character === '"') {
        quoted = false;
      } else {
        cell += character;
      }
    } else if (character === '"') {
      quoted = true;
    } else if (character === delimiter) {
      row.push(cell);
      cell = '';
    } else if (character === '\n' || character === '\r') {
      row.push(cell);
      rows.push(row);
      row = [];
      cell = '';
      if (character === '\r' && source[index + 1] === '\n') index += 1;
    } else {
      cell += character;
    }
  }
  if (quoted) throw new Error('引用符が閉じられていません。');
  if (cell || row.length) {
    row.push(cell);
    rows.push(row);
  }
  return rows.filter((values) => values.some((value) => value.trim()));
}

function parseBoolean(value: string, label: string): boolean {
  const normalized = value.trim().toLocaleLowerCase();
  if (['true', '1', 'yes', 'はい'].includes(normalized)) return true;
  if (['false', '0', 'no', 'いいえ', ''].includes(normalized)) return false;
  throw new Error(`${label}はtrue/falseで指定してください。`);
}

export function parseRecordTransfer(text: string, delimiter: ',' | '\t'): ParsedRecordRow[] {
  const rows = parseDelimited(text, delimiter);
  if (!rows.length) return [];
  const headers = rows[0].map((header) => header.trim().toLocaleLowerCase());
  const missing = recordTransferHeaders.filter((header) => !headers.includes(header));
  if (missing.length) throw new Error(`列が不足しています: ${missing.join(', ')}`);

  return rows.slice(1).map((values, index) => {
    const rowNumber = index + 2;
    const value = (header: (typeof recordTransferHeaders)[number]) => restoreProtectedFormula(values[headers.indexOf(header)] ?? '').trim();
    try {
      const rawId = value('id');
      const id = rawId === '' ? undefined : Number(rawId);
      if (id !== undefined && (!Number.isSafeInteger(id) || id <= 0)) throw new Error('idは正の整数で指定してください。');
      const name = value('name');
      const path = value('path');
      if (!name) throw new Error('nameは必須です。');
      if (!path) throw new Error('pathは必須です。');
      const rawKind = value('kind');
      if (rawKind && rawKind !== 'file' && rawKind !== 'folder' && rawKind !== 'url' && rawKind !== 'text') throw new Error('kindはfile、folder、url、textのいずれかで指定してください。');
      const rawUseCount = value('use_count');
      const useCount = rawUseCount === '' ? 0 : Number(rawUseCount);
      if (!Number.isSafeInteger(useCount) || useCount < 0) throw new Error('use_countは0以上の整数で指定してください。');
      return {
        rowNumber,
        actualName: value('actual_name'),
        record: {
          id,
          name,
          path,
          kindHint: rawKind ? rawKind as ItemKind : undefined,
          tags: value('tags').split('|').map((tag) => tag.trim().replace(/^#/, '')).filter(Boolean),
          category: value('category') || null,
          memo: value('memo'),
          favorite: parseBoolean(value('favorite'), 'favorite'),
          excluded: parseBoolean(value('excluded'), 'excluded'),
          useCount,
          lastUsedAt: value('last_used_at') || null
        }
      };
    } catch (error) {
      return { rowNumber, actualName: value('actual_name'), error: String(error instanceof Error ? error.message : error) };
    }
  });
}
