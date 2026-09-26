import test from 'node:test';
import assert from 'node:assert/strict';

test('CSV roundtrip preserves commas, quotes, line breaks, and spreadsheet formula-like text', async () => {
  const { parseRecordTransfer, serializeRecords } = await import('./recordTransfer.ts');
  const item = {
    id: 7, name: '=SUM(A1:A2)', actualName: 'a,b.txt', path: 'C:\\tmp\\a,b.txt', kind: 'file', tags: ['one', 'two'],
    category: 'docs', memo: 'line 1\n"line 2"', favorite: true, excluded: false, useCount: 4, lastUsedAt: '2026-09-22T01:02:03Z'
  };
  const [parsed] = parseRecordTransfer(serializeRecords([item], ','), ',');
  assert.equal(parsed.error, undefined);
  assert.deepEqual(parsed.record, {
    id: 7, name: '=SUM(A1:A2)', path: 'C:\\tmp\\a,b.txt', kindHint: 'file', tags: ['one', 'two'], category: 'docs',
    memo: 'line 1\n"line 2"', favorite: true, excluded: false, useCount: 4, lastUsedAt: '2026-09-22T01:02:03Z'
  });
});

test('blank ID becomes a new record and invalid values are reported per row', async () => {
  const { parseRecordTransfer, recordTransferHeaders } = await import('./recordTransfer.ts');
  const good = ['', 'New', '', 'folder', 'C:\\missing', '', '', '', 'false', 'true', '0', ''];
  const bad = ['abc', 'Bad', '', 'file', 'C:\\bad', '', '', '', 'false', 'false', '-1', ''];
  const result = parseRecordTransfer([recordTransferHeaders.join('\t'), good.join('\t'), bad.join('\t')].join('\n'), '\t');
  assert.equal(result[0].record.id, undefined);
  assert.equal(result[0].record.kindHint, 'folder');
  assert.match(result[1].error, /id/);
});

test('URL records accept the url kind', async () => {
  const { parseRecordTransfer, recordTransferHeaders } = await import('./recordTransfer.ts');
  const row = ['', 'OpenAI', 'openai.com', 'url', 'https://openai.com/', '', '', '', 'false', 'false', '0', ''];
  const [parsed] = parseRecordTransfer([recordTransferHeaders.join('\t'), row.join('\t')].join('\n'), '\t');
  assert.equal(parsed.error, undefined);
  assert.equal(parsed.record.kindHint, 'url');
  assert.equal(parsed.record.path, 'https://openai.com/');
});

test('arbitrary text records accept the text kind', async () => {
  const { parseRecordTransfer, recordTransferHeaders } = await import('./recordTransfer.ts');
  const row = ['', 'Reference', '', 'text', 'not a valid | path', '', '', '', 'false', 'false', '0', ''];
  const parsed = parseRecordTransfer([recordTransferHeaders.join('\t'), row.join('\t')].join('\n'), '\t')[0];
  assert.equal(parsed.error, undefined);
  assert.equal(parsed.record.kindHint, 'text');
  assert.equal(parsed.record.path, 'not a valid | path');
});
