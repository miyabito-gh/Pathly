import test from 'node:test';
import assert from 'node:assert/strict';

test('search rules keep filename and path opt-in', async () => {
  const { filterItems } = await import('./search.ts');
  const items = [{ id: 1, name: '資料', actualName: 'secret.pdf', path: 'C:/private/secret.pdf', kind: 'file', tags: [], memo: '', favorite: false, useCount: 0, lastUsedAt: '2026-01-01', excluded: false }];
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

test('recent and frequent lists omit unused items and sort by their activity', async () => {
  const { filterItems } = await import('./search.ts');
  const items = [
    { id: 1, name: 'Old', actualName: 'old.txt', path: 'C:/old', kind: 'file', tags: [], memo: '', favorite: true, useCount: 2, lastUsedAt: '2026-01-01', excluded: false },
    { id: 2, name: 'Never used', actualName: 'unused.txt', path: 'C:/unused', kind: 'file', tags: [], memo: '', favorite: true, useCount: 0, excluded: false },
    { id: 3, name: 'New', actualName: 'new.txt', path: 'C:/new', kind: 'file', tags: [], memo: '', favorite: false, useCount: 5, lastUsedAt: '2026-03-01', excluded: false },
    { id: 4, name: 'Excluded', actualName: 'excluded.txt', path: 'C:/excluded', kind: 'file', tags: [], memo: '', favorite: true, useCount: 99, lastUsedAt: '2026-04-01', excluded: true }
  ];
  const options = { fileName: false, path: false };
  assert.deepEqual(filterItems(items, '', 'recent', options).map((item) => item.id), [3, 1]);
  assert.deepEqual(filterItems(items, '', 'frequent', options).map((item) => item.id), [3, 1]);
  assert.deepEqual(filterItems(items, '', 'favorites', options).map((item) => item.id), [1, 2]);
});

test('free text searches categories while # and @ explicitly filter tag and category', async () => {
  const { filterItems } = await import('./search.ts');
  const items = [
    { id: 1, name: 'Project notes', actualName: 'notes.txt', path: 'C:/notes', kind: 'file', tags: ['Exam Prep'], category: 'School Year', memo: '', favorite: false, useCount: 1, excluded: false },
    { id: 2, name: 'Exam guide', actualName: 'guide.txt', path: 'C:/guide', kind: 'file', tags: ['Reference'], category: 'Work', memo: '', favorite: false, useCount: 2, excluded: false },
    { id: 3, name: 'Other', actualName: 'other.txt', path: 'C:/other', kind: 'file', tags: ['Exam'], category: 'School Year', memo: '', favorite: false, useCount: 3, excluded: false }
  ];
  const options = { fileName: false, path: false };
  assert.deepEqual(filterItems(items, 'school', 'all', options).map((item) => item.id), [1, 3]);
  assert.deepEqual(filterItems(items, '#exam', 'all', options).map((item) => item.id), [1, 3]);
  assert.deepEqual(filterItems(items, '@school', 'all', options).map((item) => item.id), [1, 3]);
  assert.deepEqual(filterItems(items, '#exam @school project', 'all', options).map((item) => item.id), [1]);
  assert.deepEqual(filterItems(items, '#"exam prep"', 'all', options).map((item) => item.id), [1]);
  assert.deepEqual(filterItems(items, '@"school year"', 'all', options).map((item) => item.id), [1, 3]);
  assert.deepEqual(filterItems(items, '# @', 'all', options), []);
});

test('sortItems sorts by activity, name, or raw last-used timestamp without mutating input', async () => {
  const { sortItems } = await import('./search.ts');
  const items = [
    { id: 1, name: 'Zeta', actualName: '', path: '', kind: 'file', tags: [], memo: '', favorite: false, useCount: 8, lastUsedAt: '2026-03-01', rawLastUsedAt: '2026-01-01T00:00:00Z', excluded: false },
    { id: 2, name: 'Alpha', actualName: '', path: '', kind: 'file', tags: [], memo: '', favorite: false, useCount: 2, lastUsedAt: '2026-04-01', rawLastUsedAt: '2026-02-01T00:00:00Z', excluded: false }
  ];
  assert.deepEqual(sortItems(items, 'frequency').map((item) => item.id), [1, 2]);
  assert.deepEqual(sortItems(items, 'name').map((item) => item.id), [2, 1]);
  assert.deepEqual(sortItems(items, 'last-used').map((item) => item.id), [2, 1]);
  assert.deepEqual(items.map((item) => item.id), [1, 2]);
});
