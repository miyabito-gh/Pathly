import test from 'node:test';
import assert from 'node:assert/strict';

test('search rules keep filename and path opt-in', async () => {
  const { filterItems } = await import('./search.ts');
  const items = [{ id: 1, name: '資料', actualName: 'secret.pdf', path: 'C:/private/secret.pdf', kind: 'file', tags: [], memo: '', favorite: false, useCount: 0, excluded: false }];
  assert.equal(filterItems(items, 'secret', 'recent', { fileName: false, path: false }).length, 0);
  assert.equal(filterItems(items, 'secret', 'recent', { fileName: true, path: false }).length, 1);
});

test('search stays within the selected list while home searches all items', async () => {
  const { filterItems } = await import('./search.ts');
  const items = [
    { id: 1, name: 'Alpha', actualName: 'alpha.txt', path: 'C:/alpha', kind: 'file', tags: [], memo: '', favorite: true, useCount: 1, lastUsedAt: '2026-01-01', excluded: false },
    { id: 2, name: 'Beta', actualName: 'beta.txt', path: 'C:/beta', kind: 'file', tags: [], memo: '', favorite: false, useCount: 3, lastUsedAt: '2026-02-01', excluded: false }
  ];
  const options = { fileName: false, path: false };
  assert.deepEqual(filterItems(items, 'beta', 'all', options).map((item) => item.id), [2]);
  assert.deepEqual(filterItems(items, 'beta', 'favorites', options), []);
  assert.deepEqual(filterItems(items, 'beta', 'recent', options).map((item) => item.id), [2]);
});
