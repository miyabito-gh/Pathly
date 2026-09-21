import test from 'node:test';
import assert from 'node:assert/strict';

test('search rules keep filename and path opt-in', async () => {
  const { filterItems } = await import('./search.ts');
  const items = [{ id: 1, name: '資料', actualName: 'secret.pdf', path: 'C:/private/secret.pdf', kind: 'file', tags: [], memo: '', favorite: false, useCount: 0, excluded: false }];
  assert.equal(filterItems(items, 'secret', 'recent', { fileName: false, path: false }).length, 0);
  assert.equal(filterItems(items, 'secret', 'recent', { fileName: true, path: false }).length, 1);
});
