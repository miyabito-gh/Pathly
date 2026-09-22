<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { invoke, isTauri } from '@tauri-apps/api/core';
  import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { filterItems, narrowPath, type SearchOptions } from '$lib/search';
  import { sampleItems, type HomeMode, type PathItem } from '$lib/pathItem';
  import { parseRecordTransfer, serializeRecords, type ParsedRecordRow, type RecordWrite } from '$lib/recordTransfer';

  type StoredPath = {
    id: number;
    name: string;
    actual_name: string;
    path: string;
    kind: 'file' | 'folder';
    tags: string[];
    category?: string | null;
    memo: string;
    favorite: boolean;
    use_count: number;
    last_used_at?: string | null;
    excluded: boolean;
  };

  type PathValidation = {
    inputPath: string;
    normalizedPath: string;
    actualName: string;
    status: 'exists' | 'missing' | 'unavailable';
    detectedKind?: 'file' | 'folder' | null;
    message: string;
  };

  type RecordDraft = {
    id: number;
    actualName: string;
    name: string;
    path: string;
    kindHint: 'file' | 'folder';
    tags: string;
    category: string;
    memo: string;
    favorite: boolean;
    excluded: boolean;
    useCount: number;
    lastUsedAt: string;
  };

  type ImportPreviewRow = ParsedRecordRow & {
    action: '追加' | '更新' | 'エラー';
    validation?: PathValidation;
    duplicatePath?: boolean;
  };

  let items: PathItem[] = sampleItems;
  let mode: HomeMode = 'all';
  let query = '';
  let selectedId = 1;
  let sortMode: 'frequency' | 'name' = 'frequency';
  let searchHistory: string[] = [];
  let historyOpen = false;
  let manageOpen = false;
  let manageSection: 'overview' | 'records' = 'overview';
  let showOptions = false;
  let options: SearchOptions = { fileName: false, path: false };
  let toast = '';
  let searchInput: HTMLInputElement;
  let storageDirectory = '';
  let defaultStorageDirectory = '';
  let storageBusy = false;
  let storageMessage = '';
  let storageError = false;
  let dataLoading = false;
  let actionMenuId: number | null = null;
  let editItemId: number | null = null;
  let pendingDropPaths: string[] = [];
  let showItemEditor = false;
  let pickerBusy = false;
  let dropActive = false;
  let dropBusy = false;
  let draftPath = '';
  let draftName = '';
  let draftTags = '';
  let draftCategory = '';
  let draftMemo = '';
  let draftFavorite = false;
  let draftExcluded = false;
  let draftKindHint: 'file' | 'folder' = 'file';
  let draftPathValidation: PathValidation | null = null;
  let draftValidationBusy = false;
  let itemMessage = '';
  let taxonomyTags: string[] = [];
  let taxonomyCategories: string[] = [];
  let taxonomyEditor: { kind: 'tag' | 'category'; oldName: string } | null = null;
  let taxonomyDraft = '';
  let brokenPaths: { id: number; name: string; path: string }[] = [];
  let linkCheckBusy = false;
  let linkCheckMessage = '';
  let dragCandidate: { id: number; x: number; y: number; canDrag: boolean } | null = null;
  let recordQuery = '';
  let selectedRecordIds: number[] = [];
  let recordSelectionAnchor: number | null = null;
  let bulkDeleteBusy = false;
  let recordDraft: RecordDraft | null = null;
  let recordValidation: PathValidation | null = null;
  let recordValidationBusy = false;
  let recordSaving = false;
  let recordMessage = '';
  let transferDialog: 'paste' | 'preview' | null = null;
  let transferText = '';
  let transferSource = '';
  let importPreview: ImportPreviewRow[] = [];
  let importBusy = false;
  let importMessage = '';
  let listPage = 0;
  let theme: 'dark' | 'light' = 'dark';
  const listPageSize = 50;

  $: filteredItems = filterItems(items, query, mode, options);
  $: results = sortMode === 'name' ? [...filteredItems].sort((a, b) => a.name.localeCompare(b.name, 'ja')) : filteredItems;
  $: pageCount = Math.max(1, Math.ceil(results.length / listPageSize));
  $: listPage = Math.min(listPage, pageCount - 1);
  $: visibleResults = results.slice(listPage * listPageSize, (listPage + 1) * listPageSize);
  $: selected = results.find((item) => item.id === selectedId) ?? null;
  $: recordResults = items.filter((item) => {
    const term = recordQuery.trim().toLocaleLowerCase();
    if (!term) return true;
    return [String(item.id), item.name, item.actualName, item.path, ...item.tags, item.category ?? '']
      .join(' ').toLocaleLowerCase().includes(term);
  });

  function handleRowPointerDown(event: PointerEvent, item: PathItem) {
    if (event.button !== 0 || !(event.target instanceof Element) || event.target.closest('button')) return;
    dragCandidate = { id: item.id, x: event.clientX, y: event.clientY, canDrag: Boolean(event.target.closest('.item-main')) };
  }

  function handleRowPointerUp(event: PointerEvent, item: PathItem) {
    if (dragCandidate?.id === item.id) {
      dragCandidate = null;
      select(item);
      (event.currentTarget as HTMLElement).querySelector<HTMLButtonElement>('.item-main')?.focus();
    }
  }

  function handlePointerMove(event: PointerEvent) {
    if (!dragCandidate?.canDrag) return;
    if (Math.hypot(event.clientX - dragCandidate.x, event.clientY - dragCandidate.y) > 8) {
      const item = items.find((candidate) => candidate.id === dragCandidate?.id);
      dragCandidate = null;
      if (item) void startPathDrag(item);
    }
  }

  function handleGlobalPointerEnd(event: PointerEvent) {
    if (dragCandidate && !(event.target instanceof Element && event.target.closest('.item-row'))) dragCandidate = null;
  }

  function select(item: PathItem) {
    selectedId = item.id;
    actionMenuId = null;
  }

  function setTheme(nextTheme: 'dark' | 'light') {
    theme = nextTheme;
    document.documentElement.dataset.theme = nextTheme;
    try { localStorage.setItem('pathly.theme', nextTheme); } catch { /* storage may be unavailable */ }
  }

  async function toggleFavorite(item: PathItem) {
    if (!isTauri() || dataLoading) return;
    dataLoading = true;
    try {
      await invoke('update_registered_path', {
        id: item.id,
        path: item.path,
        kindHint: item.kind,
        name: item.name,
        tags: item.tags,
        category: item.category || null,
        memo: item.memo,
        favorite: !item.favorite,
        excluded: item.excluded
      });
      await refreshItems();
      toast = item.favorite ? 'お気に入りから解除しました' : 'お気に入りに登録しました';
      window.setTimeout(() => (toast = ''), 2200);
    } catch (error) {
      toast = `お気に入りを更新できませんでした: ${String(error)}`;
      window.setTimeout(() => (toast = ''), 4000);
    } finally {
      dataLoading = false;
    }
  }

  function goHome() {
    manageOpen = false;
    mode = 'all';
    query = '';
    historyOpen = false;
    showOptions = false;
    selectedId = items[0]?.id ?? 0;
  }

  function openManagement(section: 'overview' | 'records' = 'overview') {
    manageOpen = true;
    manageSection = section;
    actionMenuId = null;
  }

  function normalizePathKey(path: string) {
    const normalized = path.startsWith('\\\\?\\UNC\\')
      ? `\\\\${path.slice('\\\\?\\UNC\\'.length)}`
      : path.startsWith('\\\\?\\') ? path.slice('\\\\?\\'.length) : path;
    return normalized.replaceAll('/', '\\').toLocaleLowerCase();
  }

  function rememberSearch() {
    if (!isTauri() || !query.trim()) return;
    const term = query.trim();
    searchHistory = [term, ...searchHistory.filter((item) => item !== term)].slice(0, 10);
    try { localStorage.setItem('pathly.search-history', JSON.stringify(searchHistory)); } catch { /* storage may be unavailable */ }
  }

  function focusSelection(delta: number) {
    const rows = [...document.querySelectorAll<HTMLElement>('.item-row')];
    if (!rows.length) return;
    const activeRow = document.activeElement instanceof Element ? document.activeElement.closest<HTMLElement>('.item-row') : null;
    const index = activeRow ? rows.indexOf(activeRow) : -1;
    const nextIndex = index < 0
      ? (delta > 0 ? 0 : rows.length - 1)
      : Math.min(rows.length - 1, Math.max(0, index + delta));
    const nextItem = visibleResults[nextIndex];
    if (nextItem) select(nextItem);
    rows[nextIndex]?.querySelector<HTMLButtonElement>('.item-main')?.focus();
  }

  async function changeListPage(delta: number) {
    const nextPage = Math.min(pageCount - 1, Math.max(0, listPage + delta));
    if (nextPage === listPage) return;
    listPage = nextPage;
    actionMenuId = null;
    await tick();
    const firstItem = visibleResults[delta > 0 ? 0 : visibleResults.length - 1];
    if (firstItem) {
      select(firstItem);
      await tick();
      const rowIndex = delta > 0 ? 0 : visibleResults.length - 1;
      document.querySelectorAll<HTMLButtonElement>('.item-list .item-main')[rowIndex]?.focus();
    }
  }

  function mapStoredPath(item: StoredPath): PathItem {
    const extension = item.kind === 'file' ? item.actual_name.split('.').pop()?.toLocaleUpperCase() : undefined;
    const lastUsedAt = formatLastUsed(item.last_used_at);
    return {
      id: item.id, name: item.name, actualName: item.actual_name, path: item.path, kind: item.kind,
      extension, tags: item.tags ?? [], category: item.category, memo: item.memo ?? '', favorite: item.favorite,
      useCount: item.use_count, lastUsedAt, rawLastUsedAt: item.last_used_at, excluded: item.excluded
    };
  }

  function formatLastUsed(value?: string | null): string | undefined {
    if (!value) return undefined;
    const timestamp = /^\d+$/.test(value) ? new Date(Number(value) * 1000) : new Date(value);
    if (Number.isNaN(timestamp.getTime())) return value;
    return new Intl.DateTimeFormat('ja-JP', {
      year: 'numeric', month: '2-digit', day: '2-digit',
      hour: '2-digit', minute: '2-digit', hour12: false
    }).format(timestamp);
  }

  async function copyPath(path: string) {
    try {
      await navigator.clipboard.writeText(path);
      toast = 'パスをコピーしました';
    } catch {
      toast = 'パスをコピーできませんでした';
    }
    window.setTimeout(() => (toast = ''), 2600);
  }

  async function refreshItems() {
    if (!isTauri()) return;
    dataLoading = true;
    try {
      const stored = await invoke<StoredPath[]>('list_registered_paths');
      items = stored.map(mapStoredPath);
      if (!items.some((item) => item.id === selectedId)) selectedId = items[0]?.id ?? 0;
    } catch (error) {
      itemMessage = `登録項目を読み込めませんでした: ${String(error)}`;
    } finally {
      dataLoading = false;
    }
  }

  async function refreshTaxonomy() {
    if (!isTauri()) return;
    try {
      const taxonomy = await invoke<{ tags: string[]; categories: string[] }>('list_taxonomy');
      taxonomyTags = taxonomy.tags;
      taxonomyCategories = taxonomy.categories;
    } catch (error) {
      itemMessage = `タグ・カテゴリを読み込めませんでした: ${String(error)}`;
    }
  }

  function beginTaxonomyRename(kind: 'tag' | 'category', oldName: string) {
    taxonomyEditor = { kind, oldName };
    taxonomyDraft = oldName;
  }

  async function saveTaxonomyRename() {
    if (!taxonomyEditor || !isTauri()) return;
    const destination = taxonomyDraft.trim();
    const existingNames = taxonomyEditor.kind === 'tag' ? taxonomyTags : taxonomyCategories;
    if (!destination) {
      itemMessage = '名前を入力してください。';
      return;
    }
    if (destination !== taxonomyEditor.oldName && existingNames.includes(destination)) {
      if (!window.confirm(`「${destination}」はすでに存在します。統合して名前を変更しますか？`)) return;
    }
    try {
      if (taxonomyEditor.kind === 'tag') {
        await invoke('rename_tag', { oldTag: taxonomyEditor.oldName, newTag: destination });
      } else {
        await invoke('rename_category', { oldCategory: taxonomyEditor.oldName, newCategory: destination });
      }
      taxonomyEditor = null;
      await Promise.all([refreshItems(), refreshTaxonomy()]);
    } catch (error) {
      itemMessage = String(error);
    }
  }

  async function checkRegisteredPaths() {
    if (!isTauri() || linkCheckBusy) return;
    linkCheckBusy = true;
    linkCheckMessage = '';
    try {
      brokenPaths = await invoke<{ id: number; name: string; path: string }[]>('check_registered_paths');
      linkCheckMessage = brokenPaths.length ? `${brokenPaths.length}件のリンク切れが見つかりました。` : 'すべての登録先を確認しました。リンク切れはありません。';
    } catch (error) {
      linkCheckMessage = `リンク切れ確認に失敗しました: ${String(error)}`;
    } finally {
      linkCheckBusy = false;
    }
  }

  async function openItem(item: PathItem) {
    if (!isTauri()) {
      toast = `デスクトップ版で「${item.name}」を開けます`;
    } else {
      try {
        await invoke('open_registered_path', { id: item.id });
        await refreshItems();
        toast = `「${item.name}」を開きました`;
      } catch (error) {
        toast = `開けませんでした: ${String(error)}`;
      }
    }
    window.setTimeout(() => (toast = ''), 2600);
  }

  async function openLocation(item: PathItem) {
    if (!isTauri()) {
      toast = `デスクトップ版で保存場所を開けます: ${narrowPath(item.path)}`;
    } else {
      try {
        await invoke('open_registered_location', { id: item.id });
        toast = '保存場所を開きました';
      } catch (error) {
        toast = `保存場所を開けませんでした: ${String(error)}`;
      }
    }
    window.setTimeout(() => (toast = ''), 2600);
  }

  async function startPathDrag(item: PathItem) {
    if (!isTauri()) return;
    try {
      await invoke('start_registered_path_drag', { id: item.id });
    } catch (error) {
      toast = `ドラッグを開始できませんでした: ${String(error)}`;
      window.setTimeout(() => (toast = ''), 2600);
    }
  }

  function cancelItemEditor() {
    showItemEditor = false;
    pendingDropPaths = [];
  }

  function beginNewItem() {
    editItemId = null;
    pendingDropPaths = [];
    draftPath = '';
    draftName = '';
    draftTags = '';
    draftCategory = '';
    draftMemo = '';
    draftFavorite = false;
    draftExcluded = false;
    draftKindHint = 'file';
    draftPathValidation = null;
    itemMessage = '';
    showItemEditor = true;
  }

  function beginEditItem(item: PathItem) {
    editItemId = item.id;
    pendingDropPaths = [];
    draftPath = item.path;
    draftName = item.name;
    draftTags = item.tags.join(', ');
    draftCategory = item.category ?? '';
    draftMemo = item.memo;
    draftFavorite = item.favorite;
    draftExcluded = item.excluded;
    draftKindHint = item.kind;
    draftPathValidation = null;
    itemMessage = '';
    showItemEditor = true;
  }

  async function chooseRegistrationPath(kind: 'file' | 'folder') {
    if (!isTauri() || pickerBusy) return;
    pickerBusy = true;
    itemMessage = '';
    try {
      const selectedPath = await openDialog({
        title: kind === 'folder' ? '登録するフォルダーを選択' : '登録するファイルを選択',
        directory: kind === 'folder',
        multiple: false
      });
      if (typeof selectedPath === 'string') {
        draftPath = selectedPath;
        draftKindHint = kind;
        draftPathValidation = null;
      }
    } catch (error) {
      itemMessage = `選択ダイアログを開けませんでした: ${String(error)}`;
    } finally {
      pickerBusy = false;
    }
  }

  function readDraftTags(): string[] {
    return [...new Set(draftTags.split(',').map((tag) => tag.trim().replace(/^#/, '')).filter(Boolean))];
  }

  async function validateDraftPath() {
    if (!draftPath.trim() || draftValidationBusy) return;
    draftValidationBusy = true;
    itemMessage = '';
    try {
      draftPathValidation = await validateRecordPath(draftPath.trim(), draftKindHint);
      if (draftPathValidation.normalizedPath) draftPath = draftPathValidation.normalizedPath;
      if (draftPathValidation.detectedKind) draftKindHint = draftPathValidation.detectedKind;
    } catch (error) {
      draftPathValidation = null;
      itemMessage = String(error);
    } finally {
      draftValidationBusy = false;
    }
  }

  async function saveItem() {
    if (!isTauri() || dataLoading) return;
    itemMessage = '';
    const existing = editItemId === null ? null : items.find((item) => item.id === editItemId);
    if (!existing && items.some((item) => normalizePathKey(item.path) === normalizePathKey(draftPath.trim()))) {
      if (!window.confirm('この保存場所はすでに登録されています。同じパスを重複登録しますか？')) return;
    }
    dataLoading = true;
    try {
      const validation = await validateRecordPath(draftPath.trim(), draftKindHint);
      draftPathValidation = validation;
      const validatedPath = validation.normalizedPath || draftPath.trim();
      const validatedKind = validation.detectedKind ?? draftKindHint;
      let id = editItemId;
      if (id === null) {
        const created = await invoke<StoredPath>('register_path', {
          path: validatedPath,
          kindHint: validatedKind,
          name: draftName.trim() || null,
          tags: readDraftTags(),
          category: draftCategory.trim() || null,
          memo: draftMemo,
          favorite: draftFavorite,
          excluded: draftExcluded
        });
        id = created.id;
      } else {
        await invoke('update_registered_path', {
          id,
          path: validatedPath,
          kindHint: validatedKind,
          name: draftName.trim(),
          tags: readDraftTags(),
          category: draftCategory.trim() || null,
          memo: draftMemo,
          favorite: draftFavorite,
          excluded: draftExcluded
        });
      }
      showItemEditor = false;
      pendingDropPaths = [];
      await Promise.all([refreshItems(), refreshTaxonomy()]);
      selectedId = id;
      toast = validation.status === 'exists'
        ? '登録内容を保存しました'
        : `登録内容を保存しました。${validation.message}`;
      window.setTimeout(() => (toast = ''), validation.status === 'exists' ? 2600 : 4200);
    } catch (error) {
      itemMessage = String(error);
    } finally {
      dataLoading = false;
    }
  }

  async function registerDroppedPaths(paths: string[]) {
    if (!paths.length) return;
    pendingDropPaths = paths;
    editItemId = null;
    draftPath = paths[0];
    draftName = '';
    draftTags = '';
    draftCategory = '';
    draftMemo = '';
    draftFavorite = false;
    draftExcluded = false;
    draftKindHint = 'file';
    draftPathValidation = null;
    itemMessage = paths.length > 1 ? `${paths.length}件のパスを受け取りました。1件ずつ登録してください。` : '';
    showItemEditor = true;
  }

  async function deleteItem(item: PathItem): Promise<boolean> {
    if (!isTauri() || !window.confirm(`「${item.name}」をPathlyの登録から削除しますか？\n元ファイル自体は削除されません。`)) return false;
    try {
      await invoke('delete_registered_path', { id: item.id });
      await Promise.all([refreshItems(), refreshTaxonomy()]);
      return true;
    } catch (error) {
      itemMessage = `登録を削除できませんでした: ${String(error)}`;
      return false;
    }
  }

  async function deleteEditedItem() {
    if (editItemId === null) return;
    const item = items.find((candidate) => candidate.id === editItemId);
    if (!item || !(await deleteItem(item))) return;
    cancelItemEditor();
    itemMessage = '';
    toast = `「${item.name}」を登録解除しました`;
    window.setTimeout(() => (toast = ''), 2600);
  }

  function setVisibleRecordSelection(selected: boolean) {
    const visibleIds = recordResults.map((item) => item.id);
    selectedRecordIds = selected
      ? [...new Set([...selectedRecordIds, ...visibleIds])]
      : selectedRecordIds.filter((id) => !visibleIds.includes(id));
    if (selected && visibleIds.length) recordSelectionAnchor = visibleIds[0];
  }

  function toggleRecordSelection(id: number, selected: boolean) {
    selectedRecordIds = selected
      ? [...new Set([...selectedRecordIds, id])]
      : selectedRecordIds.filter((currentId) => currentId !== id);
    recordSelectionAnchor = id;
  }

  function handleRecordRowClick(event: MouseEvent, item: PathItem) {
    if (event.shiftKey) {
      const anchorIndex = recordResults.findIndex((candidate) => candidate.id === recordSelectionAnchor);
      const itemIndex = recordResults.findIndex((candidate) => candidate.id === item.id);
      if (anchorIndex >= 0 && itemIndex >= 0) {
        const [start, end] = [Math.min(anchorIndex, itemIndex), Math.max(anchorIndex, itemIndex)];
        const rangeIds = recordResults.slice(start, end + 1).map((candidate) => candidate.id);
        selectedRecordIds = [...new Set([...selectedRecordIds, ...rangeIds])];
      } else {
        toggleRecordSelection(item.id, true);
      }
      return;
    }
    if (event.ctrlKey || event.metaKey) {
      toggleRecordSelection(item.id, !selectedRecordIds.includes(item.id));
      return;
    }
    beginRecordEdit(item);
  }

  async function deleteSelectedRecords() {
    if (!isTauri() || bulkDeleteBusy || !selectedRecordIds.length) return;
    const ids = [...selectedRecordIds];
    const selectedItems = items.filter((item) => ids.includes(item.id));
    const names = selectedItems.slice(0, 5).map((item) => `・${item.name} (ID: ${item.id})`).join('\n');
    const more = selectedItems.length > 5 ? `\nほか ${selectedItems.length - 5} 件` : '';
    if (!window.confirm(`${ids.length}件の登録レコードを削除します。\n関連するタグ情報も削除されます。元ファイル・フォルダーは削除されません。\n\n${names}${more}`)) return;
    bulkDeleteBusy = true;
    recordMessage = '';
    try {
      await invoke('delete_registered_paths', { ids });
      if (recordDraft && ids.includes(recordDraft.id)) recordDraft = null;
      selectedRecordIds = [];
      recordSelectionAnchor = null;
      await Promise.all([refreshItems(), refreshTaxonomy()]);
      recordMessage = `${ids.length}件のレコードを削除しました。`;
    } catch (error) {
      recordMessage = `一括削除できませんでした。変更は反映されていません: ${String(error)}`;
    } finally {
      bulkDeleteBusy = false;
    }
  }

  function beginRecordEdit(item: PathItem) {
    recordDraft = {
      id: item.id,
      actualName: item.actualName,
      name: item.name,
      path: item.path,
      kindHint: item.kind,
      tags: item.tags.join(', '),
      category: item.category ?? '',
      memo: item.memo,
      favorite: item.favorite,
      excluded: item.excluded,
      useCount: item.useCount,
      lastUsedAt: item.rawLastUsedAt ?? ''
    };
    recordValidation = null;
    recordMessage = '';
  }

  async function validateRecordPath(path: string, kindHint?: 'file' | 'folder'): Promise<PathValidation> {
    if (!isTauri()) throw new Error('パス検証はデスクトップ版で利用できます。');
    return invoke<PathValidation>('validate_path', { path, kindHint: kindHint ?? null });
  }

  async function validateRecordDraft() {
    if (!recordDraft || recordValidationBusy) return;
    recordValidationBusy = true;
    recordMessage = '';
    try {
      recordValidation = await validateRecordPath(recordDraft.path.trim(), recordDraft.kindHint);
      if (recordValidation.normalizedPath) recordDraft.path = recordValidation.normalizedPath;
      if (recordValidation.detectedKind) recordDraft.kindHint = recordValidation.detectedKind;
    } catch (error) {
      recordValidation = null;
      recordMessage = String(error);
    } finally {
      recordValidationBusy = false;
    }
  }

  function recordWriteFromDraft(draft: RecordDraft): RecordWrite {
    return {
      id: draft.id,
      name: draft.name.trim(),
      path: draft.path.trim(),
      kindHint: draft.kindHint,
      tags: [...new Set(draft.tags.split(',').map((tag) => tag.trim().replace(/^#/, '')).filter(Boolean))],
      category: draft.category.trim() || null,
      memo: draft.memo,
      favorite: draft.favorite,
      excluded: draft.excluded,
      useCount: Math.max(0, Math.trunc(Number(draft.useCount) || 0)),
      lastUsedAt: draft.lastUsedAt.trim() || null
    };
  }

  async function saveRecordDraft() {
    if (!recordDraft || recordSaving || !isTauri()) return;
    if (!recordDraft.name.trim() || !recordDraft.path.trim()) {
      recordMessage = '名前とパスを入力してください。';
      return;
    }
    recordSaving = true;
    recordMessage = '';
    try {
      const validation = await validateRecordPath(recordDraft.path.trim(), recordDraft.kindHint);
      recordValidation = validation;
      const record = recordWriteFromDraft({
        ...recordDraft,
        path: validation.normalizedPath || recordDraft.path,
        kindHint: validation.detectedKind ?? recordDraft.kindHint
      });
      await invoke<StoredPath[]>('apply_record_batch', { records: [record] });
      await Promise.all([refreshItems(), refreshTaxonomy()]);
      const refreshed = items.find((item) => item.id === recordDraft?.id);
      if (refreshed) beginRecordEdit(refreshed);
      recordMessage = validation.status === 'exists'
        ? 'レコードを保存しました。'
        : `レコードを保存しました。${validation.message}`;
    } catch (error) {
      recordMessage = `保存できませんでした: ${String(error)}`;
    } finally {
      recordSaving = false;
    }
  }

  async function copyRecordsAsTsv() {
    try {
      await navigator.clipboard.writeText(serializeRecords(recordResults, '\t'));
      toast = `${recordResults.length}件をExcel向け形式でコピーしました`;
    } catch (error) {
      toast = `コピーできませんでした: ${String(error)}`;
    }
    window.setTimeout(() => (toast = ''), 2600);
  }

  async function exportRecordsCsv() {
    if (!isTauri()) return;
    const content = `\uFEFF${serializeRecords(recordResults, ',')}`;
    try {
      const path = await saveDialog({
        title: 'レコードをCSVで保存',
        defaultPath: `pathly-records-${new Date().toISOString().slice(0, 10)}.csv`,
        filters: [{ name: 'CSV', extensions: ['csv'] }]
      });
      if (!path) return;
      await invoke('write_records_text_file', { path, content });
      toast = `${recordResults.length}件をCSVへ保存しました`;
    } catch (error) {
      toast = `CSVを保存できませんでした: ${String(error)}`;
    }
    window.setTimeout(() => (toast = ''), 2600);
  }

  async function backupDatabase() {
    if (!isTauri()) return;
    const timestamp = new Date().toISOString().replaceAll(':', '-').replaceAll('.', '-');
    try {
      const path = await saveDialog({
        title: 'Pathlyデータベースをバックアップ',
        defaultPath: `pathly-backup-${timestamp}.sqlite3`,
        filters: [{ name: 'SQLiteデータベース', extensions: ['sqlite3'] }]
      });
      if (!path) return;
      await invoke('backup_database', { path });
      toast = 'データベースをバックアップしました';
    } catch (error) {
      toast = `バックアップできませんでした: ${String(error)}`;
    }
    window.setTimeout(() => (toast = ''), 4000);
  }

  function openPasteDialog() {
    transferText = '';
    transferSource = 'Excel / TSV';
    importMessage = '';
    importPreview = [];
    transferDialog = 'paste';
  }

  async function importRecordsCsv() {
    if (!isTauri()) return;
    try {
      const path = await openDialog({ title: '取り込むCSVを選択', multiple: false, filters: [{ name: 'CSV', extensions: ['csv'] }] });
      if (typeof path !== 'string') return;
      transferText = await invoke<string>('read_records_text_file', { path });
      transferSource = path.split(/[\\/]/).pop() ?? path;
      await buildImportPreview(',', transferText);
    } catch (error) {
      importMessage = `CSVを読み込めませんでした: ${String(error)}`;
      transferDialog = 'preview';
    }
  }

  async function buildImportPreview(delimiter: ',' | '\t', text: string) {
    importBusy = true;
    importMessage = '';
    transferDialog = 'preview';
    try {
      const parsed = parseRecordTransfer(text, delimiter);
      const existingIds = new Set(items.map((item) => item.id));
      const idCounts = new Map<number, number>();
      for (const row of parsed) {
        if (row.record?.id !== undefined) idCounts.set(row.record.id, (idCounts.get(row.record.id) ?? 0) + 1);
      }
      importPreview = await Promise.all(parsed.map(async (row): Promise<ImportPreviewRow> => {
        if (!row.record) return { ...row, action: 'エラー' };
        if (row.record.id !== undefined && !existingIds.has(row.record.id)) {
          return { ...row, action: 'エラー', error: `ID ${row.record.id} は存在しません。更新対象は既存IDで指定してください。` };
        }
        if (row.record.id !== undefined && (idCounts.get(row.record.id) ?? 0) > 1) {
          return { ...row, action: 'エラー', error: `ID ${row.record.id} が複数行にあります。` };
        }
        try {
          const validation = await validateRecordPath(row.record.path, row.record.kindHint);
          if (!row.record.kindHint && !validation.detectedKind) {
            return { ...row, validation, action: 'エラー', error: 'パスを確認できないためkindの指定が必要です。' };
          }
          row.record.path = validation.normalizedPath || row.record.path;
          row.record.kindHint = validation.detectedKind ?? row.record.kindHint;
          const duplicatePath = items.some((item) => item.id !== row.record?.id && normalizePathKey(item.path) === normalizePathKey(row.record?.path ?? ''));
          return { ...row, validation, duplicatePath, action: row.record.id === undefined ? '追加' : '更新' };
        } catch (error) {
          return { ...row, action: 'エラー', error: String(error) };
        }
      }));
      if (!importPreview.length) importMessage = '取り込めるデータ行がありません。';
    } catch (error) {
      importPreview = [];
      importMessage = String(error);
    } finally {
      importBusy = false;
    }
  }

  async function applyImportPreview() {
    if (importBusy || importPreview.some((row) => row.action === 'エラー') || !importPreview.length || !isTauri()) return;
    importBusy = true;
    importMessage = '';
    try {
      const records = importPreview.flatMap((row) => row.record ? [row.record] : []);
      await invoke<StoredPath[]>('apply_record_batch', { records });
      await Promise.all([refreshItems(), refreshTaxonomy()]);
      const added = records.filter((record) => record.id === undefined).length;
      importMessage = `${added}件を追加し、${records.length - added}件を更新しました。`;
      importPreview = [];
    } catch (error) {
      importMessage = `反映できませんでした。変更は適用されていません: ${String(error)}`;
    } finally {
      importBusy = false;
    }
  }

  async function loadStorageSettings() {
    if (!isTauri()) {
      storageMessage = '保存先の設定はデスクトップ版で利用できます。';
      storageError = false;
      return;
    }
    try {
      const settings = await invoke<{ directory: string; default_directory: string }>('get_storage_settings');
      storageDirectory = settings.directory;
      defaultStorageDirectory = settings.default_directory;
      storageMessage = '';
      storageError = false;
    } catch (error) {
      storageMessage = `保存先設定を読み込めませんでした: ${String(error)}`;
      storageError = true;
    }
  }

  async function saveStorageDirectory() {
    if (!isTauri() || storageBusy) return;
    storageBusy = true;
    storageMessage = '';
    storageError = false;
    try {
      const settings = await invoke<{ directory: string; default_directory: string }>('set_storage_directory', { directory: storageDirectory });
      storageDirectory = settings.directory;
      defaultStorageDirectory = settings.default_directory;
      storageMessage = '保存先を変更しました。既存データも新しいフォルダーへ複製されています。';
    } catch (error) {
      storageMessage = String(error);
      storageError = true;
    } finally {
      storageBusy = false;
    }
  }

  async function resetStorageDirectory() {
    if (!isTauri() || storageBusy) return;
    storageBusy = true;
    storageMessage = '';
    storageError = false;
    try {
      const settings = await invoke<{ directory: string; default_directory: string }>('reset_storage_directory');
      storageDirectory = settings.directory;
      defaultStorageDirectory = settings.default_directory;
      storageMessage = '既定の保存先に戻しました。既存データも複製されています。';
    } catch (error) {
      storageMessage = String(error);
      storageError = true;
    } finally {
      storageBusy = false;
    }
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape' && actionMenuId !== null) {
      actionMenuId = null;
      return;
    }
    if (showItemEditor || taxonomyEditor) {
      if (event.key === 'Escape') {
        showItemEditor = false;
        taxonomyEditor = null;
      }
      return;
    }
    const target = event.target instanceof HTMLElement ? event.target : null;
    const editingText = target?.closest('input, textarea, select, [contenteditable="true"]');
    if ((event.key === 'ArrowLeft' || event.key === 'ArrowRight') && !event.altKey && !event.ctrlKey && !event.metaKey && !manageOpen && !transferDialog && !editingText) {
      event.preventDefault();
      void changeListPage(event.key === 'ArrowRight' ? 1 : -1);
    } else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
      event.preventDefault();
      searchInput?.focus();
    } else if (event.key === 'ArrowDown' && (document.activeElement === searchInput || (document.activeElement as HTMLElement)?.closest('.item-row'))) {
      event.preventDefault();
      if (historyOpen) { historyOpen = false; searchInput?.focus(); }
      else focusSelection(1);
    } else if (event.key === 'ArrowUp' && (document.activeElement === searchInput || (document.activeElement as HTMLElement)?.closest('.item-row'))) {
      event.preventDefault();
      focusSelection(-1);
    } else if (event.key === 'Enter' && selected && (event.target === searchInput || (event.target as HTMLElement).classList.contains('item-main'))) {
      event.preventDefault();
      rememberSearch();
      openItem(selected);
    } else if (event.key === 'Escape') {
      query = '';
      searchInput?.blur();
    }
  }

  onMount(() => {
    let disposed = false;
    let unlistenDrop: (() => void) | undefined;
    window.addEventListener('keydown', onKeydown);
    window.addEventListener('pointermove', handlePointerMove);
    window.addEventListener('pointerup', handleGlobalPointerEnd, true);
    window.addEventListener('pointercancel', handleGlobalPointerEnd, true);
    if (isTauri()) {
      void getCurrentWebviewWindow().onDragDropEvent(({ payload }) => {
        if (payload.type === 'enter' || payload.type === 'over') dropActive = true;
        else if (payload.type === 'leave') dropActive = false;
        else if (payload.type === 'drop') {
          dropActive = false;
          void registerDroppedPaths(payload.paths);
        }
      }).then((unlisten) => {
        if (disposed) unlisten();
        else unlistenDrop = unlisten;
      }).catch((error) => {
        toast = `ドラッグ&ドロップを準備できませんでした: ${String(error)}`;
      });
    }
    try {
      const storedTheme = localStorage.getItem('pathly.theme');
      if (storedTheme === 'light' || storedTheme === 'dark') theme = storedTheme;
    } catch { /* storage may be unavailable */ }
    document.documentElement.dataset.theme = theme;
    void refreshItems();
    void refreshTaxonomy();
    void loadStorageSettings();
    try {
      const history = JSON.parse(localStorage.getItem('pathly.search-history') ?? '[]');
      if (Array.isArray(history)) searchHistory = history.filter((item): item is string => typeof item === 'string').slice(0, 10);
    } catch { searchHistory = []; }
    return () => {
      disposed = true;
      unlistenDrop?.();
      window.removeEventListener('keydown', onKeydown);
      window.removeEventListener('pointermove', handlePointerMove);
      window.removeEventListener('pointerup', handleGlobalPointerEnd, true);
      window.removeEventListener('pointercancel', handleGlobalPointerEnd, true);
    };
  });
</script>

<svelte:head>
  <title>Pathly</title>
  <meta name="description" content="登録したファイルとフォルダを、名前とタグで見つけて開くランチャー" />
</svelte:head>

<div class="shell">
  <aside class="sidebar" aria-label="メインナビゲーション">
    <div class="brand" aria-label="Pathly">P</div>
    <nav>
      <button class:active={!manageOpen && mode === 'all'} class="nav-button" aria-label="ホーム" title="ホーム" onclick={goHome}>
        <span aria-hidden="true">⌂</span>
      </button>
      <button class:active={!manageOpen && mode === 'favorites'} class="nav-button" aria-label="お気に入り" title="お気に入り" onclick={() => { manageOpen = false; mode = 'favorites'; query = ''; }}>
        <span aria-hidden="true">★</span>
      </button>
      <button class:active={!manageOpen && mode === 'recent'} class="nav-button" aria-label="最近使った" title="最近使った" onclick={() => { manageOpen = false; mode = 'recent'; query = ''; }}>
        <span aria-hidden="true">◷</span>
      </button>
      <button class:active={!manageOpen && mode === 'frequent'} class="nav-button" aria-label="よく使う" title="よく使う" onclick={() => { manageOpen = false; mode = 'frequent'; query = ''; }}>
        <span aria-hidden="true">↗</span>
      </button>
    </nav>
    <button class:active={manageOpen} class="nav-button manage-button" aria-label="管理" title="管理" onclick={() => openManagement()}>
      <span aria-hidden="true">⚙</span>
    </button>
  </aside>

  <main class="workspace">
    <header class="topbar">
      <div>
        <h1>{manageOpen ? (manageSection === 'records' ? 'レコード' : '管理') : query ? '検索結果' : mode === 'all' ? 'ホーム' : mode === 'favorites' ? 'お気に入り' : mode === 'frequent' ? 'よく使う' : '最近使った'}</h1>
      </div>
      {#if !manageOpen}
        <div class="search-wrap">
          <span class="search-icon" aria-hidden="true">⌕</span>
          <input bind:this={searchInput} bind:value={query} aria-label="名前とタグを検索" placeholder="名前やタグを検索" />
          <kbd>Ctrl K</kbd>
          <button class:active={historyOpen} class="options-button" aria-label="検索履歴" aria-expanded={historyOpen} title="検索履歴" onclick={() => (historyOpen = !historyOpen)}>◷</button>
          <button class:active={showOptions} class="options-button" aria-label="検索オプション" title="検索オプション" onclick={() => (showOptions = !showOptions)}>☷</button>
          {#if historyOpen}
            <div class="history-menu" aria-label="最近の検索">
              <div class="history-menu-title">最近の検索 <span>最大10件</span></div>
              {#each searchHistory as term}
                <button class="history-item" onclick={() => { query = term; historyOpen = false; searchInput?.focus(); }}><span>⌕</span>{term}</button>
              {:else}
                <div class="history-empty">検索履歴はありません</div>
              {/each}
              {#if searchHistory.length}<button class="history-clear" onclick={() => { searchHistory = []; localStorage.removeItem('pathly.search-history'); }}>履歴を消去</button>{/if}
            </div>
          {/if}
        </div>
        <div class="topbar-actions">
          <button class="theme-toggle" aria-label={theme === 'dark' ? 'ライトテーマに切り替え' : 'ダークテーマに切り替え'} title={theme === 'dark' ? 'ライトテーマに切り替え' : 'ダークテーマに切り替え'} onclick={() => setTheme(theme === 'dark' ? 'light' : 'dark')}>
            {#if theme === 'dark'}<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="4"/><path d="M12 2v2m0 16v2M4.93 4.93l1.42 1.42m11.3 11.3 1.42 1.42M2 12h2m16 0h2M4.93 19.07l1.42-1.42m11.3-11.3 1.42-1.42"/></svg>{:else}<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M20.5 15.3A8.5 8.5 0 0 1 8.7 3.5 8.6 8.6 0 1 0 20.5 15.3Z"/><path d="m16.5 4 .5 1.5 1.5.5-1.5.5-.5 1.5L16 6.5 14.5 6 16 5.5z"/></svg>{/if}
          </button>
          <button class="primary topbar-add" onclick={beginNewItem}>＋ 登録</button>
        </div>
      {:else}
        <div class="topbar-actions"><button class="theme-toggle" aria-label={theme === 'dark' ? 'ライトテーマに切り替え' : 'ダークテーマに切り替え'} title={theme === 'dark' ? 'ライトテーマに切り替え' : 'ダークテーマに切り替え'} onclick={() => setTheme(theme === 'dark' ? 'light' : 'dark')}>
          {#if theme === 'dark'}<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="4"/><path d="M12 2v2m0 16v2M4.93 4.93l1.42 1.42m11.3 11.3 1.42 1.42M2 12h2m16 0h2M4.93 19.07l1.42-1.42m11.3-11.3 1.42-1.42"/></svg>{:else}<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M20.5 15.3A8.5 8.5 0 0 1 8.7 3.5 8.6 8.6 0 1 0 20.5 15.3Z"/><path d="m16.5 4 .5 1.5 1.5.5-1.5.5-.5 1.5L16 6.5 14.5 6 16 5.5z"/></svg>{/if}
        </button></div>
      {/if}
    </header>

    {#if manageOpen}
      <div class="management-switcher" aria-label="管理メニュー">
        <button class:current={manageSection === 'overview'} onclick={() => (manageSection = 'overview')}>設定</button>
        <button class:current={manageSection === 'records'} onclick={() => (manageSection = 'records')}>レコード</button>
      </div>
      {#if manageSection === 'overview'}
      <section class="management" aria-labelledby="management-title">
        <div class="management-heading"><div class="empty-icon" aria-hidden="true">⚙</div><div><h2 id="management-title">管理</h2></div><button class="primary management-add" onclick={beginNewItem}>＋ 登録</button></div>
        <section class="settings-card item-management" aria-labelledby="items-title">
          <div class="settings-card-heading"><div><h3 id="items-title">登録項目 <span class="subtle-count">{items.length}件</span></h3></div></div>
          {#if items.length}
            <div class="managed-list">
              {#each items as item (item.id)}
                <div class="managed-row">
                  <div class="managed-type" aria-hidden="true">{item.kind === 'folder' ? 'DIR' : item.extension}</div>
                  <div class="managed-info"><strong>{item.name}</strong><span title={item.path}>{narrowPath(item.path, 64)}</span><div class="tags">{#each item.tags as tag}<span>#{tag}</span>{/each}{#if item.excluded}<span class="excluded-tag">検索対象外</span>{/if}</div></div>
                  <button class="text-button" onclick={() => beginEditItem(item)}>編集</button>
                </div>
              {/each}
            </div>
          {:else}
            <div class="managed-empty">{dataLoading ? '読み込み中…' : '登録項目はまだありません。'}</div>
          {/if}
          {#if itemMessage}<p class="storage-message error" role="status">{itemMessage}</p>{/if}
        </section>
        <section class="settings-card taxonomy-card" aria-labelledby="taxonomy-title">
          <div class="settings-card-heading"><div><h3 id="taxonomy-title">タグ・カテゴリ</h3></div></div>
          <div class="taxonomy-group"><h4>タグ <span class="subtle-count">{taxonomyTags.length}</span></h4><div class="taxonomy-chips">{#each taxonomyTags as tag}<button onclick={() => beginTaxonomyRename('tag', tag)} title={`「${tag}」を変更`}>#{tag}<span>編集</span></button>{:else}<span class="taxonomy-empty">登録済みタグはありません</span>{/each}</div></div>
          <div class="taxonomy-group"><h4>カテゴリ <span class="subtle-count">{taxonomyCategories.length}</span></h4><div class="taxonomy-chips">{#each taxonomyCategories as category}<button onclick={() => beginTaxonomyRename('category', category)} title={`「${category}」を変更`}>{category}<span>編集</span></button>{:else}<span class="taxonomy-empty">登録済みカテゴリはありません</span>{/each}</div></div>
        </section>
        <section class="settings-card link-check-card" aria-labelledby="link-check-title">
          <div class="settings-card-heading"><div><h3 id="link-check-title">リンク切れ確認</h3></div><button class="secondary" disabled={linkCheckBusy} onclick={checkRegisteredPaths}>{linkCheckBusy ? '確認中…' : '今すぐ確認'}</button></div>
          {#if linkCheckMessage}<p class="storage-message" role="status">{linkCheckMessage}</p>{/if}
          {#if brokenPaths.length}<div class="broken-list">{#each brokenPaths as broken}<div><strong>{broken.name}</strong><span title={broken.path}>{narrowPath(broken.path, 72)}</span><button class="text-button" onclick={() => { const item = items.find((candidate) => candidate.id === broken.id); if (item) beginEditItem(item); }}>確認</button></div>{/each}</div>{/if}
        </section>
        <section class="settings-card" aria-labelledby="storage-title">
          <div class="settings-card-heading"><div><h3 id="storage-title">データ保存先</h3></div><span class="setting-badge">SQLite</span></div>
          <label class="storage-label" for="storage-directory">保存先フォルダー</label>
          <div class="storage-input-row"><input id="storage-directory" bind:value={storageDirectory} placeholder="例: D:\PathlyData" spellcheck="false" /><button class="primary" disabled={storageBusy || !storageDirectory.trim()} onclick={saveStorageDirectory}>{storageBusy ? '保存中…' : '変更する'}</button></div>
          <div class="default-location"><span>既定</span><code>{defaultStorageDirectory || '読み込み中…'}</code><button class="text-button" disabled={storageBusy} onclick={resetStorageDirectory}>既定に戻す</button></div>
          <p class="storage-note">開発時はプロジェクトのトップ、ビルド版はPathly.exeと同じフォルダーを既定にします。保存先変更時は現在のデータベースを複製します。変更前のデータベースは自動削除しません。</p>
          {#if storageMessage}<p class:error={storageError} class="storage-message" role="status">{storageMessage}</p>{/if}
        </section>
      </section>
      {:else}
      <section class="records-management" aria-labelledby="records-title">
        <div class="records-toolbar">
          <div><h2 id="records-title">レコード</h2></div>
          <div class="records-actions">
            <button class="secondary danger-button" disabled={!selectedRecordIds.length || bulkDeleteBusy} onclick={() => void deleteSelectedRecords()}>{bulkDeleteBusy ? '削除中…' : `選択を削除 (${selectedRecordIds.length})`}</button>
            <button class="secondary" onclick={() => void copyRecordsAsTsv()}>Excelへコピー</button>
            <button class="secondary" onclick={openPasteDialog}>Excelから貼り付け</button>
            <button class="secondary" onclick={() => void exportRecordsCsv()}>CSV書き出し</button>
            <button class="secondary" onclick={() => void importRecordsCsv()}>CSV読み込み</button>
            <button class="secondary backup-action" onclick={() => void backupDatabase()}>DBバックアップ</button>
          </div>
        </div>
        <div class="record-search-row">
          <span aria-hidden="true">⌕</span>
          <input bind:value={recordQuery} aria-label="レコードを検索" placeholder="ID、名前、ファイル名、パス、タグ、カテゴリを検索" />
          <span>{#if selectedRecordIds.length}<strong>{selectedRecordIds.length}件選択・</strong>{/if}{recordResults.length} / {items.length}件</span>
        </div>
        {#if recordMessage}<p class:success={recordMessage.includes('削除しました')} class="record-message" role="status">{recordMessage}</p>{/if}
        <div class="records-grid">
          <section class="records-table-card" aria-label="レコード一覧">
            <div class="records-table-head"><input type="checkbox" aria-label="表示中のレコードをすべて選択" checked={recordResults.length > 0 && recordResults.every((item) => selectedRecordIds.includes(item.id))} onchange={(event) => setVisibleRecordSelection(event.currentTarget.checked)} /><span>ID</span><span>名前</span><span>種別</span><span>パス</span><span>操作</span></div>
            <div class="records-table-body">
              {#each recordResults as item (item.id)}
                <div class:active={recordDraft?.id === item.id} class:bulk-selected={selectedRecordIds.includes(item.id)} class="record-table-line">
                  <input type="checkbox" aria-label={`${item.name}を選択`} checked={selectedRecordIds.includes(item.id)} onchange={(event) => toggleRecordSelection(item.id, event.currentTarget.checked)} />
                  <button class="record-table-row" onclick={(event) => handleRecordRowClick(event, item)}>
                    <span class="record-id">{item.id}</span>
                    <span class="record-name"><strong>{item.name}</strong><small>{item.actualName}</small></span>
                    <span>{item.kind === 'folder' ? 'フォルダー' : 'ファイル'}</span>
                    <span class="record-path" title={item.path}>{item.path}</span>
                    <span class="record-edit-label">編集</span>
                  </button>
                </div>
              {:else}
                <div class="records-empty">該当するレコードはありません。</div>
              {/each}
            </div>
          </section>

          <aside class="record-editor" aria-label="レコード編集">
            {#if recordDraft}
              <div class="record-editor-heading"><div><span>レコードID</span><strong>{recordDraft.id}</strong></div><button class="text-button danger-button" onclick={async () => { const item = items.find((candidate) => candidate.id === recordDraft?.id); if (item && await deleteItem(item)) recordDraft = null; }}>削除</button></div>
              <div class="record-form">
                <label>実ファイル名 <span>読み取り専用</span><input value={recordDraft.actualName} readonly /></label>
                <label>表示名<input bind:value={recordDraft.name} /></label>
                <label>パス<textarea bind:value={recordDraft.path} rows="3" spellcheck="false" oninput={() => (recordValidation = null)}></textarea></label>
                <div class="validation-row"><button class="secondary" disabled={recordValidationBusy || !recordDraft.path.trim()} onclick={() => void validateRecordDraft()}>{recordValidationBusy ? '確認中…' : 'パスを確認'}</button>
                  {#if recordValidation}<span class:ok={recordValidation.status === 'exists'} class:warning={recordValidation.status !== 'exists'}>{recordValidation.message}</span>{/if}
                </div>
                <label>種別 <span>{recordValidation?.status === 'exists' ? 'パスから判定' : 'パスが見つからない場合に使用'}</span><select bind:value={recordDraft.kindHint} disabled={recordValidation?.status === 'exists'}><option value="file">ファイル</option><option value="folder">フォルダー</option></select></label>
                <label>タグ <span>カンマ区切り</span><input bind:value={recordDraft.tags} /></label>
                <label>カテゴリ<input bind:value={recordDraft.category} /></label>
                <label>メモ <span>検索対象外</span><textarea bind:value={recordDraft.memo} rows="3"></textarea></label>
                <div class="record-form-split"><label>利用回数<input type="number" min="0" step="1" bind:value={recordDraft.useCount} /></label><label>最終利用日時 <span>ISO 8601</span><input bind:value={recordDraft.lastUsedAt} placeholder="2026-09-22T12:00:00Z" /></label></div>
                <div class="form-checks"><label><input type="checkbox" bind:checked={recordDraft.favorite} /> お気に入り</label><label><input type="checkbox" bind:checked={recordDraft.excluded} /> 検索対象外</label></div>
                {#if recordMessage}<p class:success={recordMessage.startsWith('レコードを保存')} class="record-message" role="status">{recordMessage}</p>{/if}
                <div class="record-save-actions"><button class="primary" disabled={recordSaving || !recordDraft.name.trim() || !recordDraft.path.trim()} onclick={() => void saveRecordDraft()}>{recordSaving ? '保存中…' : '変更を保存'}</button></div>
              </div>
            {:else}
              <div class="record-editor-empty"><span aria-hidden="true">▤</span><strong>編集するレコードを選択</strong><p>一覧の行を選ぶと、保存値を直接確認できます。</p></div>
            {/if}
          </aside>
        </div>
        <p class="records-note">IDがある取込行だけを更新し、空のIDは新規追加します。パス一致による上書きは行いません。実ファイル自体を削除・変更することもありません。</p>
      </section>
      {/if}
    {:else}
      {#if showOptions}
        <div class="options-panel" aria-label="検索オプション">
          <span>追加の検索対象</span>
          <label><input type="checkbox" bind:checked={options.fileName} /> ファイル名</label>
          <label><input type="checkbox" bind:checked={options.path} /> 保存場所</label>
          <label class="sort-control">並び順<select bind:value={sortMode}><option value="frequency">利用回数順</option><option value="name">名前順</option></select></label>
        </div>
      {/if}

      <div class="content-grid">
        <section class="list-panel" aria-label="登録項目一覧">
          {#if results.length}
            <div class="list-head"><span>項目</span><span>保存場所</span><span class="sr-only">操作</span></div>
            <div class="item-list">
              {#each visibleResults as item (item.id)}
                <article class:selected={selected?.id === item.id} class="item-row" onpointerdown={(event) => handleRowPointerDown(event, item)} onpointerup={(event) => handleRowPointerUp(event, item)}>
                  <button class="item-main" aria-label={`${item.name}を開く`} onclick={(event) => { event.stopPropagation(); select(item); void openItem(item); }}>
                    <span class:item-folder={item.kind === 'folder'} class="type-badge">{item.kind === 'folder' ? 'DIR' : item.extension}</span>
                    <span class="item-copy"><strong>{item.name}</strong><span class="tags">{#each item.tags as tag}<span>#{tag}</span>{/each}</span></span>
                  </button>
                  <span class="path" title={item.path}>{narrowPath(item.path)}</span>
                  <button class:favorite-active={item.favorite} class="icon-button action-icon-button favorite-toggle" aria-label={item.favorite ? `${item.name}をお気に入りから解除` : `${item.name}をお気に入りに登録`} title={item.favorite ? 'お気に入り解除' : 'お気に入り登録'} onclick={(event) => { event.stopPropagation(); void toggleFavorite(item); }}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="m12 3 2.8 5.7 6.2.9-4.5 4.4 1.1 6.2-5.6-3-5.6 3 1.1-6.2L3 9.6l6.2-.9z"/></svg></button>
                  <button class="icon-button action-icon-button" aria-label={`${item.name}の場所を開く`} title="場所を開く" onclick={(event) => { event.stopPropagation(); openLocation(item); }}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 7.5h7l2 2h9v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><path d="M3 7V5a2 2 0 0 1 2-2h5l2 2h5"/></svg></button>
                  <button class="icon-button subdued" aria-label={`${item.name}のその他の操作`} title="その他の操作" aria-expanded={actionMenuId === item.id} onclick={(event) => { event.stopPropagation(); select(item); actionMenuId = actionMenuId === item.id ? null : item.id; }}>•••</button>
                  {#if actionMenuId === item.id}
                    <div class="item-context-menu" aria-label={`${item.name}の操作`}>
                      <button onclick={(event) => { event.stopPropagation(); actionMenuId = null; void openItem(item); }}>開く</button>
                      <button onclick={(event) => { event.stopPropagation(); actionMenuId = null; void openLocation(item); }}>保存場所を開く</button>
                      <button onclick={(event) => { event.stopPropagation(); actionMenuId = null; beginEditItem(item); }}>編集</button>
                    </div>
                  {/if}
                </article>
              {/each}
            </div>
            <div class="list-pagination">
              <span>{listPage * listPageSize + 1}–{Math.min((listPage + 1) * listPageSize, results.length)} / {results.length}件</span>
              <div><button class="pagination-button" disabled={listPage === 0} aria-label="前のページ" onclick={() => void changeListPage(-1)}>‹</button><span>{listPage + 1} / {pageCount}</span><button class="pagination-button" disabled={listPage >= pageCount - 1} aria-label="次のページ" onclick={() => void changeListPage(1)}>›</button></div>
            </div>
          {:else}
            <div class="empty-state"><div class="empty-icon">⌕</div><h2>見つかりません</h2><p>名前やタグを変えるか、検索オプションを開いてください。</p></div>
          {/if}
        </section>

        <aside class="detail-panel" aria-label="選択項目の詳細">
          {#if selected}
            <div class="detail-top"><span class="detail-label">選択中</span><span class="status-dot">登録済み</span></div>
            <div class="detail-title"><span class:item-folder={selected.kind === 'folder'} class="type-badge large">{selected.kind === 'folder' ? 'DIR' : selected.extension}</span><h2>{selected.name}</h2></div>
            <div class="detail-actions" aria-label="項目の操作">
              <button class:favorite-active={selected.favorite} class="icon-button action-icon-button favorite-toggle" aria-label={selected.favorite ? 'お気に入りから解除' : 'お気に入りに登録'} title={selected.favorite ? 'お気に入り解除' : 'お気に入り登録'} onclick={() => void toggleFavorite(selected)}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="m12 3 2.8 5.7 6.2.9-4.5 4.4 1.1 6.2-5.6-3-5.6 3 1.1-6.2L3 9.6l6.2-.9z"/></svg></button>
              <button class="icon-button action-icon-button detail-open" aria-label={`${selected.name}を開く`} title="開く (Enter)" onclick={() => openItem(selected)}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M14 3h7v7M10 14 21 3M19 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2h6" /></svg></button>
              <button class="icon-button action-icon-button" aria-label="保存場所を開く" title="保存場所を開く" onclick={() => openLocation(selected)}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 7.5h7l2 2h9v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><path d="M3 7V5a2 2 0 0 1 2-2h5l2 2h5"/></svg></button>
              <button class="icon-button action-icon-button" aria-label="編集" title="編集" onclick={() => beginEditItem(selected)}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="m14 5 5 5M4 20l4.2-.9L19 8.3a2.1 2.1 0 0 0-3-3L5.2 16.1z"/><path d="M13 20h8"/></svg></button>
              <button class="icon-button action-icon-button" aria-label="パスをコピー" title="パスをコピー" onclick={() => copyPath(selected.path)}><svg viewBox="0 0 24 24" aria-hidden="true"><rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h3"/></svg></button>
            </div>
            <dl>
              <div><dt>タグ</dt><dd class="detail-tags">{#each selected.tags as tag}<span>#{tag}</span>{/each}</dd></div>
              <div><dt>ファイル名</dt><dd>{selected.actualName}</dd></div>
              {#if selected.category}<div><dt>カテゴリ</dt><dd>{selected.category}</dd></div>{/if}
              <div><dt>保存場所</dt><dd class="detail-path" title={selected.path}>{narrowPath(selected.path, 70)}</dd></div>
              <div><dt>最終利用日時</dt><dd>{selected.lastUsedAt ?? '利用履歴なし'}</dd></div>
              <div><dt>利用回数</dt><dd>{selected.useCount}回</dd></div>
              {#if selected.memo}<div><dt>メモ</dt><dd>{selected.memo}</dd></div>{/if}
            </dl>
          {:else}
            <div class="detail-empty">項目を選択すると詳細を表示します。</div>
          {/if}
        </aside>
      </div>
    {/if}
  </main>

  {#if showItemEditor}
    <div class="dialog-scrim" role="presentation" onclick={(event) => { if (event.target === event.currentTarget) showItemEditor = false; }}>
          <div class="item-dialog" role="dialog" aria-modal="true" aria-labelledby="item-dialog-title" tabindex="-1">
        <div class="dialog-heading"><div><div class="eyebrow">登録項目</div><h2 id="item-dialog-title">{editItemId === null ? 'ファイル・フォルダーを登録' : '登録内容を編集'}</h2></div><button class="icon-button" aria-label="閉じる" onclick={cancelItemEditor}>×</button></div>
        {#if pendingDropPaths.length > 1}<p class="storage-note">複数のパスをドロップしました。1件ずつ登録するため、現在のパスを登録した後に残りを再度ドロップしてください。</p>{/if}
        <form onsubmit={(event) => { event.preventDefault(); void saveItem(); }}>
          <label for="draft-path">ファイルまたはフォルダーのパス</label>
          <input id="draft-path" bind:value={draftPath} oninput={() => (draftPathValidation = null)} placeholder="例: C:\\Users\\name\\Documents\\report.pdf" required spellcheck="false" />
          <div class="path-picker-actions"><button class="secondary" type="button" disabled={pickerBusy} onclick={() => chooseRegistrationPath('file')}>ファイルを選ぶ</button><button class="secondary" type="button" disabled={pickerBusy} onclick={() => chooseRegistrationPath('folder')}>フォルダーを選ぶ</button></div>
          <div class="registration-validation"><button class="secondary" type="button" disabled={draftValidationBusy || !draftPath.trim()} onclick={() => void validateDraftPath()}>{draftValidationBusy ? '確認中…' : 'パスを確認'}</button>{#if draftPathValidation}<span class:ok={draftPathValidation.status === 'exists'}>{draftPathValidation.message}</span>{/if}</div>
          <label for="draft-kind">種別 <span>{draftPathValidation?.status === 'exists' ? 'パスから判定' : 'パスが見つからない場合に使用'}</span></label>
          <select id="draft-kind" class="dialog-select" bind:value={draftKindHint} disabled={draftPathValidation?.status === 'exists'}><option value="file">ファイル</option><option value="folder">フォルダー</option></select>
          <label for="draft-name">名前</label>
          <input id="draft-name" bind:value={draftName} placeholder="空欄ならファイル名から作成" />
          <div class="form-columns"><div><label for="draft-tags">タグ <span>カンマ区切り</span></label><input id="draft-tags" bind:value={draftTags} placeholder="例: 企画, 月次" list="known-tags" /><datalist id="known-tags">{#each taxonomyTags as tag}<option value={tag}></option>{/each}</datalist></div><div><label for="draft-category">カテゴリ <span>1つまで</span></label><input id="draft-category" bind:value={draftCategory} placeholder="任意" list="known-categories" /><datalist id="known-categories">{#each taxonomyCategories as category}<option value={category}></option>{/each}</datalist></div></div>
          <label for="draft-memo">メモ <span>検索対象外</span></label>
          <textarea id="draft-memo" bind:value={draftMemo} rows="3" placeholder="補足情報"></textarea>
          <div class="form-checks"><label><input type="checkbox" bind:checked={draftFavorite} /> お気に入り</label><label><input type="checkbox" bind:checked={draftExcluded} /> 検索対象外</label></div>
          {#if itemMessage}<p class="storage-message error" role="status">{itemMessage}</p>{/if}
          <div class="dialog-actions">{#if editItemId !== null}<button class="secondary danger-button dialog-unregister" type="button" disabled={dataLoading} onclick={() => void deleteEditedItem()}>登録解除</button>{/if}<button class="secondary" type="button" onclick={cancelItemEditor}>キャンセル</button><button class="primary" type="submit" disabled={dataLoading}>{dataLoading ? '保存中…' : '保存する'}</button></div>
        </form>
          </div>
    </div>
  {/if}

  {#if taxonomyEditor}
    <div class="dialog-scrim" role="presentation" onclick={(event) => { if (event.target === event.currentTarget) taxonomyEditor = null; }}>
      <div class="item-dialog taxonomy-dialog" role="dialog" aria-modal="true" aria-labelledby="taxonomy-dialog-title" tabindex="-1">
        <div class="dialog-heading"><div><div class="eyebrow">一括変更</div><h2 id="taxonomy-dialog-title">{taxonomyEditor.kind === 'tag' ? 'タグ名を変更' : 'カテゴリ名を変更'}</h2></div><button class="icon-button" aria-label="閉じる" onclick={() => (taxonomyEditor = null)}>×</button></div>
        <label for="taxonomy-new-name">新しい名前</label><input id="taxonomy-new-name" bind:value={taxonomyDraft} />
        <p class="storage-note">該当するすべての登録項目に反映します。</p>
        <div class="dialog-actions"><button class="secondary" onclick={() => (taxonomyEditor = null)}>キャンセル</button><button class="primary" disabled={!taxonomyDraft.trim()} onclick={saveTaxonomyRename}>変更する</button></div>
      </div>
    </div>
  {/if}

  {#if transferDialog === 'paste'}
    <div class="dialog-scrim" role="presentation" onclick={(event) => { if (event.target === event.currentTarget) transferDialog = null; }}>
      <div class="item-dialog transfer-dialog" role="dialog" aria-modal="true" aria-labelledby="paste-dialog-title" tabindex="-1">
        <div class="dialog-heading"><div><div class="eyebrow">Excel / TSV</div><h2 id="paste-dialog-title">レコードを貼り付け</h2></div><button class="icon-button" aria-label="閉じる" onclick={() => (transferDialog = null)}>×</button></div>
        <p class="transfer-help">「Excelへコピー」で取得したヘッダー付きの表を貼り付けてください。IDが空の行は追加、既存IDの行は更新として確認します。</p>
        <textarea class="transfer-textarea" bind:value={transferText} rows="12" placeholder="id&#9;name&#9;actual_name&#9;kind&#9;path…" spellcheck="false"></textarea>
        {#if importMessage}<p class="record-message" role="status">{importMessage}</p>{/if}
        <div class="dialog-actions"><button class="secondary" onclick={() => (transferDialog = null)}>キャンセル</button><button class="primary" disabled={!transferText.trim() || importBusy} onclick={() => { transferSource = 'Excel / TSV'; void buildImportPreview('\t', transferText); }}>{importBusy ? '確認中…' : '内容を確認'}</button></div>
      </div>
    </div>
  {/if}

  {#if transferDialog === 'preview'}
    <div class="dialog-scrim" role="presentation" onclick={(event) => { if (event.target === event.currentTarget && !importBusy) transferDialog = null; }}>
      <div class="item-dialog transfer-dialog preview-dialog" role="dialog" aria-modal="true" aria-labelledby="preview-dialog-title" tabindex="-1">
        <div class="dialog-heading"><div><div class="eyebrow">{transferSource || 'インポート'}</div><h2 id="preview-dialog-title">反映内容を確認</h2></div><button class="icon-button" aria-label="閉じる" disabled={importBusy} onclick={() => (transferDialog = null)}>×</button></div>
        {#if importBusy}<div class="import-loading">パスとレコードを確認しています…</div>{/if}
        {#if importMessage}<p class="record-message" role="status">{importMessage}</p>{/if}
        {#if importPreview.length}
          <div class="preview-summary"><span>追加 {importPreview.filter((row) => row.action === '追加').length}件</span><span>更新 {importPreview.filter((row) => row.action === '更新').length}件</span><span class:error={importPreview.some((row) => row.action === 'エラー')}>エラー {importPreview.filter((row) => row.action === 'エラー').length}件</span></div>
          <div class="import-preview-table">
            <div class="import-preview-head"><span>行</span><span>処理</span><span>ID / 名前</span><span>パス確認</span></div>
            {#each importPreview as row}
              <div class:error-row={row.action === 'エラー'} class="import-preview-row">
                <span>{row.rowNumber}</span>
                <span class:preview-error={row.action === 'エラー'} class="preview-action">{row.action}</span>
                <span><strong>{row.record?.id ?? '新規'} / {row.record?.name ?? '—'}</strong>{#if row.error}<small>{row.error}</small>{/if}</span>
                <span>{row.validation?.status === 'exists' ? '存在を確認' : row.validation?.message ?? '未確認'}{#if row.duplicatePath}<small class="duplicate-warning">同じパスのレコードがあります（登録可）</small>{/if}</span>
              </div>
            {/each}
          </div>
          <p class="transfer-help">actual_nameは参照用で、取込時はパスから再判定します。1行でもエラーがある場合は反映できません。</p>
        {/if}
        <div class="dialog-actions"><button class="secondary" disabled={importBusy} onclick={() => (transferDialog = null)}>閉じる</button><button class="primary" disabled={importBusy || !importPreview.length || importPreview.some((row) => row.action === 'エラー')} onclick={() => void applyImportPreview()}>{importBusy ? '処理中…' : 'すべて反映'}</button></div>
      </div>
    </div>
  {/if}

  {#if toast}<div class="toast" role="status">{toast}</div>{/if}
  {#if dropActive}<div class="drop-overlay" aria-live="polite"><div><span aria-hidden="true">＋</span><strong>ここにドロップして登録</strong><small>ファイル・フォルダーをそのまま追加できます</small></div></div>{/if}
</div>
