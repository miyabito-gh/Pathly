<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke, isTauri } from '@tauri-apps/api/core';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { filterItems, narrowPath, type SearchOptions } from '$lib/search';
  import { sampleItems, type HomeMode, type PathItem } from '$lib/pathItem';

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

  let items: PathItem[] = sampleItems;
  let mode: HomeMode = 'all';
  let query = '';
  let selectedId = 1;
  let sortMode: 'frequency' | 'name' = 'frequency';
  let searchHistory: string[] = [];
  let historyOpen = false;
  let manageOpen = false;
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
  let itemMessage = '';
  let taxonomyTags: string[] = [];
  let taxonomyCategories: string[] = [];
  let taxonomyEditor: { kind: 'tag' | 'category'; oldName: string } | null = null;
  let taxonomyDraft = '';
  let brokenPaths: { id: number; name: string; path: string }[] = [];
  let linkCheckBusy = false;
  let linkCheckMessage = '';
  let dragCandidate: { id: number; x: number; y: number; canDrag: boolean } | null = null;

  $: filteredItems = filterItems(items, query, mode, options);
  $: results = sortMode === 'name' ? [...filteredItems].sort((a, b) => a.name.localeCompare(b.name, 'ja')) : filteredItems;
  $: selected = results.find((item) => item.id === selectedId) ?? null;

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

  function goHome() {
    manageOpen = false;
    mode = 'all';
    query = '';
    historyOpen = false;
    showOptions = false;
    selectedId = items[0]?.id ?? 0;
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
    const nextItem = results[nextIndex];
    if (nextItem) select(nextItem);
    rows[nextIndex]?.focus();
  }

  function mapStoredPath(item: StoredPath): PathItem {
    const extension = item.kind === 'file' ? item.actual_name.split('.').pop()?.toLocaleUpperCase() : undefined;
    const lastUsedAt = formatLastUsed(item.last_used_at);
    return {
      id: item.id, name: item.name, actualName: item.actual_name, path: item.path, kind: item.kind,
      extension, tags: item.tags ?? [], category: item.category, memo: item.memo ?? '', favorite: item.favorite,
      useCount: item.use_count, lastUsedAt, excluded: item.excluded
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
      if (typeof selectedPath === 'string') draftPath = selectedPath;
    } catch (error) {
      itemMessage = `選択ダイアログを開けませんでした: ${String(error)}`;
    } finally {
      pickerBusy = false;
    }
  }

  function readDraftTags(): string[] {
    return [...new Set(draftTags.split(',').map((tag) => tag.trim().replace(/^#/, '')).filter(Boolean))];
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
      let id = editItemId;
      if (id === null) {
        const created = await invoke<StoredPath>('register_path', {
          path: draftPath.trim(),
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
          path: draftPath.trim(),
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
    itemMessage = paths.length > 1 ? `${paths.length}件のパスを受け取りました。1件ずつ登録してください。` : '';
    showItemEditor = true;
  }

  async function deleteItem(item: PathItem) {
    if (!isTauri() || !window.confirm(`「${item.name}」をPathlyの登録から削除しますか？\n元ファイル自体は削除されません。`)) return;
    try {
      await invoke('delete_registered_path', { id: item.id });
      await Promise.all([refreshItems(), refreshTaxonomy()]);
    } catch (error) {
      itemMessage = `登録を削除できませんでした: ${String(error)}`;
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
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
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
    <button class:active={manageOpen} class="nav-button manage-button" aria-label="管理" title="管理" onclick={() => (manageOpen = true)}>
      <span aria-hidden="true">⚙</span>
    </button>
  </aside>

  <main class="workspace">
    <header class="topbar">
      <div>
        <h1>{manageOpen ? '管理' : query ? '検索結果' : mode === 'all' ? 'ホーム' : mode === 'favorites' ? 'お気に入り' : mode === 'frequent' ? 'よく使う' : '最近使った'}</h1>
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
        <button class="primary topbar-add" onclick={beginNewItem}>＋ 登録</button>
      {/if}
    </header>

    {#if manageOpen}
      <section class="management" aria-labelledby="management-title">
        <div class="management-heading"><div class="empty-icon" aria-hidden="true">⚙</div><div><h2 id="management-title">管理</h2><p>登録項目とアプリの保存先を管理します。</p></div><button class="primary management-add" onclick={beginNewItem}>＋ 登録</button></div>
        <section class="settings-card item-management" aria-labelledby="items-title">
          <div class="settings-card-heading"><div><h3 id="items-title">登録項目 <span class="subtle-count">{items.length}件</span></h3><p>登録・編集・検索対象の切替</p></div></div>
          {#if items.length}
            <div class="managed-list">
              {#each items as item (item.id)}
                <div class="managed-row">
                  <div class="managed-type" aria-hidden="true">{item.kind === 'folder' ? 'DIR' : item.extension}</div>
                  <div class="managed-info"><strong>{item.name}</strong><span title={item.path}>{narrowPath(item.path, 64)}</span><div class="tags">{#each item.tags as tag}<span>#{tag}</span>{/each}{#if item.excluded}<span class="excluded-tag">検索対象外</span>{/if}</div></div>
                  <button class="text-button" onclick={() => beginEditItem(item)}>編集</button>
                  <button class="text-button danger-button" onclick={() => deleteItem(item)}>削除</button>
                </div>
              {/each}
            </div>
          {:else}
            <div class="managed-empty">{dataLoading ? '読み込み中…' : '登録項目はまだありません。'}</div>
          {/if}
          {#if itemMessage}<p class="storage-message error" role="status">{itemMessage}</p>{/if}
        </section>
        <section class="settings-card taxonomy-card" aria-labelledby="taxonomy-title">
          <div class="settings-card-heading"><div><h3 id="taxonomy-title">タグ・カテゴリ</h3><p>登録内容の整理。項目の編集時に候補として使えます。</p></div></div>
          <div class="taxonomy-group"><h4>タグ <span class="subtle-count">{taxonomyTags.length}</span></h4><div class="taxonomy-chips">{#each taxonomyTags as tag}<button onclick={() => beginTaxonomyRename('tag', tag)} title={`「${tag}」を変更`}>#{tag}<span>編集</span></button>{:else}<span class="taxonomy-empty">登録済みタグはありません</span>{/each}</div></div>
          <div class="taxonomy-group"><h4>カテゴリ <span class="subtle-count">{taxonomyCategories.length}</span></h4><div class="taxonomy-chips">{#each taxonomyCategories as category}<button onclick={() => beginTaxonomyRename('category', category)} title={`「${category}」を変更`}>{category}<span>編集</span></button>{:else}<span class="taxonomy-empty">登録済みカテゴリはありません</span>{/each}</div></div>
        </section>
        <section class="settings-card link-check-card" aria-labelledby="link-check-title">
          <div class="settings-card-heading"><div><h3 id="link-check-title">リンク切れ確認</h3><p>自動確認は行いません。必要なときだけ実行してください。</p></div><button class="secondary" disabled={linkCheckBusy} onclick={checkRegisteredPaths}>{linkCheckBusy ? '確認中…' : '今すぐ確認'}</button></div>
          {#if linkCheckMessage}<p class="storage-message" role="status">{linkCheckMessage}</p>{/if}
          {#if brokenPaths.length}<div class="broken-list">{#each brokenPaths as broken}<div><strong>{broken.name}</strong><span title={broken.path}>{narrowPath(broken.path, 72)}</span><button class="text-button" onclick={() => { const item = items.find((candidate) => candidate.id === broken.id); if (item) beginEditItem(item); }}>確認</button></div>{/each}</div>{/if}
        </section>
        <section class="settings-card" aria-labelledby="storage-title">
          <div class="settings-card-heading"><div><h3 id="storage-title">データ保存先</h3><p>Pathlyのデータベースを置くフォルダー</p></div><span class="setting-badge">SQLite</span></div>
          <label class="storage-label" for="storage-directory">保存先フォルダー</label>
          <div class="storage-input-row"><input id="storage-directory" bind:value={storageDirectory} placeholder="例: D:\PathlyData" spellcheck="false" /><button class="primary" disabled={storageBusy || !storageDirectory.trim()} onclick={saveStorageDirectory}>{storageBusy ? '保存中…' : '変更する'}</button></div>
          <div class="default-location"><span>既定</span><code>{defaultStorageDirectory || '読み込み中…'}</code><button class="text-button" disabled={storageBusy} onclick={resetStorageDirectory}>既定に戻す</button></div>
          <p class="storage-note">開発時はプロジェクトのトップ、ビルド版はPathly.exeと同じフォルダーを既定にします。保存先変更時は現在のデータベースを複製します。変更前のデータベースは自動削除しません。</p>
          {#if storageMessage}<p class:error={storageError} class="storage-message" role="status">{storageMessage}</p>{/if}
        </section>
        <div class="management-note">確認を実行したときだけ、登録済みの各パスへアクセスします。自動走査やPC全体のインデックス作成は行いません。</div>
      </section>
    {:else}
      {#if showOptions}
        <div class="options-panel" aria-label="検索オプション">
          <span>追加の検索対象</span>
          <label><input type="checkbox" bind:checked={options.fileName} /> ファイル名</label>
          <label><input type="checkbox" bind:checked={options.path} /> 保存場所</label>
          <label class="sort-control">並び順<select bind:value={sortMode}><option value="frequency">利用回数順</option><option value="name">名前順</option></select></label>
          <span class="option-hint">通常は名前とタグだけを検索します</span>
        </div>
      {/if}

      <div class="list-summary"><span>{mode === 'all' ? 'すべての登録項目' : mode === 'favorites' ? 'お気に入り' : mode === 'frequent' ? 'よく使う' : '最近使った'}</span><span class="result-count">{results.length}件</span></div>

      <div class="content-grid">
        <section class="list-panel" aria-label="登録項目一覧">
          {#if results.length}
            <div class="list-head"><span>項目</span><span>保存場所</span><span class="sr-only">操作</span></div>
            <div class="item-list">
              {#each results as item (item.id)}
                <article class:selected={selected?.id === item.id} class="item-row" onpointerdown={(event) => handleRowPointerDown(event, item)} onpointerup={(event) => handleRowPointerUp(event, item)}>
                  <button class="item-main" aria-label={`${item.name}を開く`} onclick={(event) => { event.stopPropagation(); select(item); void openItem(item); }}>
                    <span class:item-folder={item.kind === 'folder'} class="type-badge">{item.kind === 'folder' ? 'DIR' : item.extension}</span>
                    <span class="item-copy"><strong>{item.name}</strong><span class="tags">{#each item.tags as tag}<span>#{tag}</span>{/each}</span></span>
                  </button>
                  <span class="path" title={item.path}>{narrowPath(item.path)}</span>
                  <button class="icon-button" aria-label={`${item.name}の場所を開く`} title="場所を開く" onclick={(event) => { event.stopPropagation(); openLocation(item); }}>↗</button>
                  <button class="icon-button subdued" aria-label={`${item.name}のその他の操作`} title="その他の操作" aria-expanded={actionMenuId === item.id} onclick={(event) => { event.stopPropagation(); select(item); actionMenuId = actionMenuId === item.id ? null : item.id; }}>•••</button>
                  {#if actionMenuId === item.id}
                    <div class="item-context-menu" aria-label={`${item.name}の操作`}>
                      <button onclick={(event) => { event.stopPropagation(); actionMenuId = null; void openItem(item); }}>開く</button>
                      <button onclick={(event) => { event.stopPropagation(); actionMenuId = null; void openLocation(item); }}>保存場所を開く</button>
                      <button onclick={(event) => { event.stopPropagation(); actionMenuId = null; beginEditItem(item); }}>編集</button>
                      <button class="danger-button" onclick={(event) => { event.stopPropagation(); actionMenuId = null; void deleteItem(item); }}>登録解除</button>
                    </div>
                  {/if}
                </article>
              {/each}
            </div>
            <div class="keyboard-hint"><kbd>↑</kbd><kbd>↓</kbd> 選択　 <kbd>Enter</kbd> 開く　 <kbd>Ctrl K</kbd> 検索</div>
          {:else}
            <div class="empty-state"><div class="empty-icon">⌕</div><h2>見つかりません</h2><p>名前やタグを変えるか、検索オプションを開いてください。</p></div>
          {/if}
        </section>

        <aside class="detail-panel" aria-label="選択項目の詳細">
          {#if selected}
            <div class="detail-top"><span class="detail-label">選択中</span><span class="status-dot">登録済み</span></div>
            <div class="detail-title"><span class:item-folder={selected.kind === 'folder'} class="type-badge large">{selected.kind === 'folder' ? 'DIR' : selected.extension}</span><h2>{selected.name}</h2></div>
            <div class="detail-actions"><button class="primary" onclick={() => openItem(selected)}>開く <span>Enter</span></button><button class="secondary" onclick={() => openLocation(selected)} aria-label="保存場所を開く">場所を開く</button></div>
            <dl>
              <div><dt>タグ</dt><dd class="detail-tags">{#each selected.tags as tag}<span>#{tag}</span>{/each}</dd></div>
              <div><dt>ファイル名</dt><dd>{selected.actualName}</dd></div>
              {#if selected.category}<div><dt>カテゴリ</dt><dd>{selected.category}</dd></div>{/if}
              <div><dt>保存場所</dt><dd class="detail-path" title={selected.path}>{narrowPath(selected.path, 70)}</dd></div>
              <div><dt>最終利用日時</dt><dd>{selected.lastUsedAt ?? '利用履歴なし'}</dd></div>
              <div><dt>利用回数</dt><dd>{selected.useCount}回</dd></div>
              {#if selected.memo}<div><dt>メモ</dt><dd>{selected.memo}</dd></div>{/if}
            </dl>
            <div class="detail-footer"><button class="copy-path-button" aria-label="パスをコピー" title="パスをコピー" onclick={() => copyPath(selected.path)}>⧉</button></div>
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
          <input id="draft-path" bind:value={draftPath} placeholder="例: C:\\Users\\name\\Documents\\report.pdf" required spellcheck="false" />
          <div class="path-picker-actions"><button class="secondary" type="button" disabled={pickerBusy} onclick={() => chooseRegistrationPath('file')}>ファイルを選ぶ</button><button class="secondary" type="button" disabled={pickerBusy} onclick={() => chooseRegistrationPath('folder')}>フォルダーを選ぶ</button></div>
          <label for="draft-name">名前</label>
          <input id="draft-name" bind:value={draftName} placeholder="空欄ならファイル名から作成" />
          <div class="form-columns"><div><label for="draft-tags">タグ <span>カンマ区切り</span></label><input id="draft-tags" bind:value={draftTags} placeholder="例: 企画, 月次" list="known-tags" /><datalist id="known-tags">{#each taxonomyTags as tag}<option value={tag}></option>{/each}</datalist></div><div><label for="draft-category">カテゴリ <span>1つまで</span></label><input id="draft-category" bind:value={draftCategory} placeholder="任意" list="known-categories" /><datalist id="known-categories">{#each taxonomyCategories as category}<option value={category}></option>{/each}</datalist></div></div>
          <label for="draft-memo">メモ <span>検索対象外</span></label>
          <textarea id="draft-memo" bind:value={draftMemo} rows="3" placeholder="補足情報"></textarea>
          <div class="form-checks"><label><input type="checkbox" bind:checked={draftFavorite} /> お気に入り</label><label><input type="checkbox" bind:checked={draftExcluded} /> 検索対象外</label></div>
          {#if itemMessage}<p class="storage-message error" role="status">{itemMessage}</p>{/if}
          <div class="dialog-actions"><button class="secondary" type="button" onclick={cancelItemEditor}>キャンセル</button><button class="primary" type="submit" disabled={dataLoading}>{dataLoading ? '保存中…' : '保存する'}</button></div>
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

  {#if toast}<div class="toast" role="status">{toast}</div>{/if}
  {#if dropActive}<div class="drop-overlay" aria-live="polite"><div><span aria-hidden="true">＋</span><strong>ここにドロップして登録</strong><small>ファイル・フォルダーをそのまま追加できます</small></div></div>{/if}
</div>
