<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { makeHistory, createMessage, type UIMessage } from './lib/history';
  import { visibleRange } from './lib/virtual';
  import { resolveShortcut } from './lib/shortcuts';

  type Theme = 'dark' | 'light';
  const ROW_HEIGHT = 88;
  const DEMO_REPLY = 'This is a simulated Codex response, rendered incrementally without a network request or a running agent. Ultima keeps the interface lightweight and puts control in your hands.';
  const projects = ['ultima', 'sibambo', 'mind-place'];
  let theme: Theme = 'dark';
  let messages: UIMessage[] = makeHistory();
  let draft = '';
  let search = '';
  let activeProject = 'ultima';
  let navOpen = false;
  let streaming = false;
  let scrollTop = 0;
  let viewportHeight = 600;
  let listEl: HTMLDivElement;
  let composerEl: HTMLTextAreaElement;
  let searchEl: HTMLInputElement;
  let streamTimer: number | undefined;
  let measureObserver: ResizeObserver | undefined;

  $: range = visibleRange(messages.length, scrollTop, viewportHeight, ROW_HEIGHT);
  $: visibleMessages = messages.slice(range.start, range.end);
  $: filteredProjects = projects.filter((project) => project.includes(search.trim().toLowerCase()));

  function applyTheme(value: Theme) {
    theme = value;
    document.documentElement.dataset.theme = value;
    try { localStorage.setItem('ultima-prototype-theme', value); } catch { /* preview works when storage is disabled */ }
  }

  function toggleTheme() {
    applyTheme(theme === 'dark' ? 'light' : 'dark');
  }

  function scrollToLatest() {
    if (listEl) {
      listEl.scrollTop = Math.max(0, listEl.scrollHeight - listEl.clientHeight);
      scrollTop = listEl.scrollTop;
    }
  }

  function stopStream() {
    if (streamTimer !== undefined) window.clearInterval(streamTimer);
    streamTimer = undefined;
    streaming = false;
  }

  async function sendMessage() {
    const text = draft.trim();
    if (!text || streaming) return;
    draft = '';
    messages = [...messages, createMessage('user', text, messages.length), createMessage('assistant', '', messages.length + 1)];
    streaming = true;
    await tick();
    scrollToLatest();
    let written = 0;
    streamTimer = window.setInterval(() => {
      written = Math.min(DEMO_REPLY.length, written + 12);
      const last = messages[messages.length - 1];
      messages = [...messages.slice(0, -1), { ...last, text: DEMO_REPLY.slice(0, written) }];
      if (written >= DEMO_REPLY.length) stopStream();
      scrollToLatest();
    }, 45);
  }

  async function newThread() {
    stopStream();
    messages = [];
    draft = '';
    navOpen = false;
    scrollTop = 0;
    await tick();
    composerEl?.focus();
  }

  async function restoreFixture() {
    stopStream();
    messages = makeHistory();
    navOpen = false;
    await tick();
    scrollToLatest();
  }

  function handleComposerKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
      event.preventDefault();
      void sendMessage();
    }
  }

  function handleWindowKeydown(event: KeyboardEvent) {
    const action = resolveShortcut(event);
    if (!action) return;
    if (action === 'dismiss') {
      if (navOpen) { event.preventDefault(); navOpen = false; }
      return;
    }
    event.preventDefault();
    if (action === 'search') { navOpen = true; void tick().then(() => searchEl?.focus()); }
    if (action === 'composer') composerEl?.focus();
    if (action === 'new-thread') void newThread();
  }

  onMount(() => {
    let saved: string | null = null;
    try { saved = localStorage.getItem('ultima-prototype-theme'); } catch { /* continue with default */ }
    applyTheme(saved === 'light' ? 'light' : 'dark');
    if (typeof ResizeObserver !== 'undefined') {
      measureObserver = new ResizeObserver(() => { viewportHeight = listEl?.clientHeight ?? 600; });
      measureObserver.observe(listEl);
    }
    viewportHeight = listEl?.clientHeight ?? 600;
    void tick().then(scrollToLatest);
    return () => { stopStream(); measureObserver?.disconnect(); };
  });
</script>

<svelte:window onkeydown={handleWindowKeydown} />

<div class="shell">
  {#if navOpen}
    <button class="mobile-scrim" aria-label="Close navigation" onclick={() => (navOpen = false)}></button>
  {/if}
  <aside class:open={navOpen} class="sidebar" aria-label="Primary navigation">
    <div class="sidebar-head">
      <div class="brand">
        <span class="brand-mark" aria-hidden="true"><span></span><span></span><span></span><span></span></span>
        <span class="brand-name">ultima<span class="brand-period">.</span></span>
      </div>
      <span class="edition">CODEX / 01</span>
    </div>

    <button class="new-thread" onclick={() => void newThread()}>
      <span class="new-thread-symbol" aria-hidden="true">+</span>
      New thread
      <kbd>⌘ N</kbd>
    </button>

    <label class="search-box">
      <span class="search-symbol" aria-hidden="true">⌕</span>
      <span class="sr-only">Search projects</span>
      <input bind:this={searchEl} bind:value={search} placeholder="Find a project" />
      <kbd>⌘ K</kbd>
    </label>

    <div class="sidebar-group">
      <div class="group-label"><span>WORKSPACE</span><span class="group-count">03</span></div>
      {#each filteredProjects as project}
        <button class:active={activeProject === project} class="project-item" aria-current={activeProject === project ? 'page' : undefined} onclick={() => { activeProject = project; navOpen = false; }}>
          <span class="project-symbol" aria-hidden="true">{project.charAt(0).toUpperCase()}</span>
          <span>{project}</span>
          {#if activeProject === project}<span class="active-dot" aria-hidden="true"></span>{/if}
        </button>
      {:else}
        <p class="empty-search">No matching projects.</p>
      {/each}
    </div>

    <div class="sidebar-group history-links">
      <div class="group-label"><span>THIS SESSION</span><span class="group-count">MOCK</span></div>
      <button class="history-link" onclick={() => void restoreFixture()}><span class="tiny-square" aria-hidden="true"></span> 10k message stress fixture</button>
      <button class="history-link" onclick={() => void newThread()}><span class="tiny-square" aria-hidden="true"></span> Blank thread</button>
    </div>

    <div class="sidebar-bottom">
      <span class="mock-indicator"><span class="mock-dot" aria-hidden="true"></span> Interface prototype</span>
      <span class="build-label">NC-03 · CLOUD MOCK</span>
    </div>
  </aside>

  <main class="main-panel">
    <header class="topbar">
      <div class="topbar-left">
        <button class="icon-button menu-toggle" aria-label="Open navigation" aria-expanded={navOpen} onclick={() => (navOpen = !navOpen)}><span aria-hidden="true">☰</span></button>
        <span class="breadcrumb">Workspace</span>
        <span class="slash" aria-hidden="true">/</span>
        <span class="breadcrumb-current">{activeProject}</span>
        <span class="preview-label">PREVIEW</span>
      </div>
      <div class="topbar-actions">
        <span class="local-only"><span class="status-dot" aria-hidden="true"></span> No engine connected</span>
        <button class="icon-button theme-toggle" aria-label={theme === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'} title="Toggle color theme" onclick={toggleTheme}>
          {#if theme === 'dark'}<span aria-hidden="true">☀</span>{:else}<span aria-hidden="true">☾</span>{/if}
        </button>
      </div>
    </header>

    <section class="conversation" aria-labelledby="conversation-title">
      <div class="conversation-head">
        <p class="eyebrow">YOUR WORKSPACE / DEMO 001</p>
        <h1 id="conversation-title">Build with clarity<span class="heading-period">.</span></h1>
        <p class="conversation-subtitle">A quieter place to think, ship and keep moving.</p>
        <div class="conversation-meta">
          <span><span class="meta-circle" aria-hidden="true"></span> Synthetic conversation</span>
          <span class="meta-divider" aria-hidden="true"></span>
          <span data-testid="message-count">{messages.length.toLocaleString('en-US')} messages</span>
          <span class="meta-divider" aria-hidden="true"></span>
          <span>Virtualized</span>
        </div>
      </div>

      <div class="conversation-body">
        {#if messages.length === 0}
          <div class="empty-state">
            <span class="empty-orbit" aria-hidden="true">✳</span>
            <h2>Start something good.</h2>
            <p>Type a prompt below, or restore the 10k-message fixture to stress-test the UI.</p>
            <button class="secondary-action" onclick={() => void restoreFixture()}>Load 10k message fixture</button>
          </div>
        {:else}
          <div class="feed" bind:this={listEl} onscroll={(event) => (scrollTop = event.currentTarget.scrollTop)} role="feed" aria-label="Synthetic conversation history" aria-busy={streaming} tabindex="0" data-testid="message-feed">
            <div class="virtual-canvas" style:height={messages.length * ROW_HEIGHT + 'px'}>
              {#each visibleMessages as message, offset (message.id)}
                <article class:user-message={message.role === 'user'} class="message-row" style:top={(range.start + offset) * ROW_HEIGHT + 'px'} aria-posinset={range.start + offset + 1} aria-setsize={messages.length} data-testid="message-row">
                  <div class="message-avatar" aria-hidden="true">{message.role === 'user' ? 'Y' : 'U'}</div>
                  <div class="message-content">
                    <div class="message-line"><span class="message-author">{message.role === 'user' ? 'You' : 'Ultima'}</span><span class="message-time">{message.at}</span></div>
                    <p>{message.text || 'Thinking…'}</p>
                  </div>
                </article>
              {/each}
            </div>
          </div>
        {/if}
      </div>
    </section>

    <div class="composer-area">
      <div class="composer-shell" class:busy={streaming}>
        <label for="composer" class="sr-only">Message to Codex demo</label>
        <textarea id="composer" bind:this={composerEl} bind:value={draft} onkeydown={handleComposerKeydown} placeholder="Ask anything, build something…" rows="2" maxlength="8000" aria-describedby="composer-hint" disabled={streaming}></textarea>
        <div class="composer-bottom">
          <div class="composer-mode"><span class="mode-icon" aria-hidden="true">◇</span> Codex <span class="mode-arrow" aria-hidden="true">⌄</span><span class="mode-separator" aria-hidden="true"></span><span class="demo-mode">Demo only</span></div>
          {#if streaming}
            <button class="send-button stop-button" aria-label="Stop simulated response" onclick={stopStream}><span class="stop-icon" aria-hidden="true"></span></button>
          {:else}
            <button class="send-button" aria-label="Send demo message" disabled={!draft.trim()} onclick={() => void sendMessage()}><span aria-hidden="true">↑</span></button>
          {/if}
        </div>
      </div>
      <div class="under-composer"><p id="composer-hint">Enter to send · Shift + Enter for newline · Ctrl/⌘ + K to search</p><span>SIMULATION · NO NETWORK</span></div>
    </div>
  </main>
</div>
