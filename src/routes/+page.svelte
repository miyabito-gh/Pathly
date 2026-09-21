<script lang="ts">
  import { onMount } from 'svelte';
  import { filterItems, narrowPath, type SearchOptions } from '$lib/search';
  import { sampleItems, type HomeMode, type PathItem } from '$lib/pathItem';

  let items: PathItem[] = sampleItems;
  let mode: HomeMode = 'recent';
  let query = '';
  let selectedId = 1;
  let manageOpen = false;
  let showOptions = false;
  let options: SearchOptions = { fileName: false, path: false };
  let toast = '';
  let searchInput: HTMLInputElement;

  $: results = filterItems(items, query, mode, options);
  $: selected = results.find((item) => item.id === selectedId) ?? results[0] ?? null;

  function select(item: PathItem) {
    selectedId = item.id;
  }

  function openItem(item: PathItem) {
    items = items.map((candidate) => candidate.id === item.id
      ? { ...candidate, useCount: candidate.useCount + 1, lastUsedAt: 'たった今' }
      : candidate);
    toast = `「${item.name}」を開きます`;
    window.setTimeout(() => (toast = ''), 1800);
  }

  function openLocation(item: PathItem) {
    toast = `保存場所を開きます: ${narrowPath(item.path)}`;
    window.setTimeout(() => (toast = ''), 1800);
  }

  function startDrag(event: DragEvent, item: PathItem) {
    event.dataTransfer?.setData('text/plain', item.path);
    event.dataTransfer?.setData('text/uri-list', `file:///${item.path.replaceAll('\\', '/')}`);
    if (event.dataTransfer) event.dataTransfer.effectAllowed = 'copy';
  }

  function moveSelection(delta: number) {
    if (!results.length) return;
    const index = Math.max(0, results.findIndex((item) => item.id === selectedId));
    const next = results[Math.min(results.length - 1, Math.max(0, index + delta))];
    if (next) selectedId = next.id;
  }

  function onKeydown(event: KeyboardEvent) {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
      event.preventDefault();
      searchInput?.focus();
    } else if (event.key === 'ArrowDown' && document.activeElement === searchInput) {
      event.preventDefault();
      moveSelection(1);
    } else if (event.key === 'ArrowUp' && document.activeElement === searchInput) {
      event.preventDefault();
      moveSelection(-1);
    } else if (event.key === 'Enter' && selected) {
      event.preventDefault();
      openItem(selected);
    } else if (event.key === 'Escape') {
      query = '';
      searchInput?.blur();
    }
  }

  onMount(() => {
    window.addEventListener('keydown', onKeydown);
    return () => window.removeEventListener('keydown', onKeydown);
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
      <button class:active={!manageOpen} class="nav-button" aria-label="ホーム" title="ホーム" onclick={() => (manageOpen = false)}>
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
        <div class="eyebrow">登録した場所だけを、すぐに開く</div>
        <h1>{manageOpen ? '管理' : query ? '検索結果' : mode === 'favorites' ? 'お気に入り' : mode === 'frequent' ? 'よく使う' : '最近使った'}</h1>
      </div>
      {#if !manageOpen}
        <div class="search-wrap">
          <span class="search-icon" aria-hidden="true">⌕</span>
          <input bind:this={searchInput} bind:value={query} aria-label="名前とタグを検索" placeholder="名前やタグを検索" />
          <kbd>Ctrl K</kbd>
          <button class:active={showOptions} class="options-button" aria-label="検索オプション" title="検索オプション" onclick={() => (showOptions = !showOptions)}>☷</button>
        </div>
      {/if}
    </header>

    {#if manageOpen}
      <section class="management" aria-labelledby="management-title">
        <div class="empty-icon" aria-hidden="true">⚙</div>
        <h2 id="management-title">登録項目の管理</h2>
        <p>登録・編集・タグ整理・リンク切れ確認は、この画面にまとめます。</p>
        <button class="primary" onclick={() => (toast = '登録ダイアログは次の実装で追加します')}>＋ 登録する</button>
        <div class="management-note">自動走査やPC全体のインデックス作成は行いません。</div>
      </section>
    {:else}
      {#if showOptions}
        <div class="options-panel" aria-label="検索オプション">
          <span>追加の検索対象</span>
          <label><input type="checkbox" bind:checked={options.fileName} /> ファイル名</label>
          <label><input type="checkbox" bind:checked={options.path} /> 保存場所</label>
          <span class="option-hint">通常は名前とタグだけを検索します</span>
        </div>
      {/if}

      <div class="home-switcher" role="tablist" aria-label="ホーム表示">
        <button class:current={mode === 'favorites' && !query} role="tab" aria-selected={mode === 'favorites' && !query} onclick={() => { mode = 'favorites'; query = ''; }}>お気に入り</button>
        <button class:current={mode === 'recent' && !query} role="tab" aria-selected={mode === 'recent' && !query} onclick={() => { mode = 'recent'; query = ''; }}>最近使った</button>
        <button class:current={mode === 'frequent' && !query} role="tab" aria-selected={mode === 'frequent' && !query} onclick={() => { mode = 'frequent'; query = ''; }}>よく使う</button>
        <span class="result-count">{results.length}件</span>
      </div>

      <div class="content-grid">
        <section class="list-panel" aria-label="登録項目一覧">
          {#if results.length}
            <div class="list-head"><span>項目</span><span>保存場所</span><span class="sr-only">操作</span></div>
            <div class="item-list">
              {#each results as item (item.id)}
                <article class:selected={selected?.id === item.id} class="item-row" draggable="true" ondragstart={(event) => startDrag(event, item)}>
                  <button class="item-main" aria-label={`${item.name}を選択`} onclick={(event) => { event.stopPropagation(); select(item); }}>
                    <span class:item-folder={item.kind === 'folder'} class="type-badge">{item.kind === 'folder' ? 'DIR' : item.extension}</span>
                    <span class="item-copy"><strong>{item.name}</strong><span class="tags">{#each item.tags as tag}<span>#{tag}</span>{/each}</span></span>
                  </button>
                  <span class="path" title={item.path}>{narrowPath(item.path)}</span>
                  <button class="icon-button" aria-label={`${item.name}の場所を開く`} title="場所を開く" onclick={(event) => { event.stopPropagation(); openLocation(item); }}>↗</button>
                  <button class="icon-button subdued" aria-label={`${item.name}のその他の操作`} title="その他の操作" onclick={(event) => { event.stopPropagation(); select(item); }}>•••</button>
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
              <div><dt>保存場所</dt><dd class="detail-path" title={selected.path}>{narrowPath(selected.path, 70)}</dd></div>
              <div><dt>最終利用</dt><dd>{selected.lastUsedAt ?? '未使用'}</dd></div>
              <div><dt>利用回数</dt><dd>{selected.useCount}回</dd></div>
              {#if selected.memo}<div><dt>メモ</dt><dd>{selected.memo}</dd></div>{/if}
            </dl>
            <div class="detail-footer">一覧から外へドラッグできます</div>
          {:else}
            <div class="detail-empty">項目を選択すると詳細を表示します。</div>
          {/if}
        </aside>
      </div>
    {/if}
  </main>

  {#if toast}<div class="toast" role="status">{toast}</div>{/if}
</div>
