<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { createMessage, type UIMessage } from './lib/history';
  import { visibleRange } from './lib/virtual';
  import { resolveShortcut } from './lib/shortcuts';
  import { nativeBridge, desktopAvailable, type EngineBridge, type EngineEvent } from './lib/desktop-bridge';
  import { EngineSessionFence } from './lib/engine-session-fence';
  import { addThread, appendDelta, appendProject, cappedMessages, restoreProjects, restoreThreads, restoreThreadMessages, saveMetadata, type Project, type Thread } from './lib/desktop-state';

  const ROW_HEIGHT = 88;
  let projects: Project[] = [];
  let threads: Thread[] = [];
  let activeProject = '';
  let activeThread = '';
  let activeTurn = '';
  let pendingPrompt = '';
  let messages: UIMessage[] = [];
  let projectInput = '';
  let executable = '';
  let draft = '';
  let model = '';
  let models: string[] = [];
  let status: 'disconnected' | 'connecting' | 'disconnecting' | 'ready' | 'error' = 'disconnected';
  let busy = false;
  let errorMessage = '';
  let search = '';
  let navOpen = false;
  let theme: 'dark' | 'light' = 'dark';
  let scrollTop = 0;
  let viewportHeight = 600;
  let feedEl: HTMLDivElement;
  let composerEl: HTMLTextAreaElement;
  let searchEl: HTMLInputElement;
  let bridge: EngineBridge | null = null;
  let unlisten: (() => void) | null = null;
  let counter = 0;
  const pending = new Map<number, 'models' | 'create' | 'resume' | 'turn' | 'interrupt'>();
  // IPC notifications can arrive before invoke() resolves; replay only bounded replies.
  const earlyReplies = new Map<number, Extract<EngineEvent, { type: 'reply' }>>();
  const fence = new EngineSessionFence<EngineEvent>();
  let connectionEpoch = 0;
  $: shownProjects = projects.filter(p => p.name.toLowerCase().includes(search.toLowerCase()));
  $: shownThreads = threads.filter(t => t.project === activeProject && t.title.toLowerCase().includes(search.toLowerCase()));
  $: range = visibleRange(messages.length, scrollTop, viewportHeight, ROW_HEIGHT);
  $: shownMessages = messages.slice(range.start, range.end);
  $: selectedProject = projects.find(p => p.path === activeProject);

  function persist() {
    try {
      saveMetadata((key, value) => localStorage.setItem(key, value), projects, threads);
      localStorage.setItem('ultima-codex-executable-v1', executable);
    } catch { /* disabled storage is supported */ }
  }
  function setTheme(next: 'light' | 'dark') {
    theme = next;
    document.documentElement.dataset.theme = next;
    try { localStorage.setItem('ultima-prototype-theme', next); } catch { /* no persistence */ }
  }
  function scrollLatest() {
    if (!feedEl) return;
    feedEl.scrollTop = Math.max(0, feedEl.scrollHeight - feedEl.clientHeight);
    scrollTop = feedEl.scrollTop;
  }
  function measureFeed(node: HTMLDivElement) {
    const update = () => { viewportHeight = node.clientHeight; };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(node);
    return { destroy: () => observer.disconnect() };
  }
  function addProject() {
    const updated = appendProject(projects, projectInput);
    if (updated === projects || updated.length === projects.length) {
      const duplicate = updated.some(p => p.path.toLowerCase() === projectInput.trim().toLowerCase());
      if (!duplicate) { errorMessage = 'Enter an absolute project directory.'; return; }
    }
    projects = updated;
    if (!activeProject) activeProject = updated[0]?.path ?? activeProject;
    projectInput = '';
    persist();
  }
  async function disconnect() {
    if (status === 'disconnecting') return;
    if (status === 'connecting') {
      errorMessage = 'Connection is still being established.';
      return;
    }
    const epoch = ++connectionEpoch;
    fence.beginDisconnect();
    pending.clear();
    earlyReplies.clear();
    activeTurn = '';
    busy = false;
    status = 'disconnecting';
    try {
      await bridge?.disconnect();
      if (epoch === connectionEpoch) status = 'disconnected';
    } catch {
      if (epoch === connectionEpoch) {
        status = 'error';
        errorMessage = 'Codex shutdown was not confirmed. Reconnect is blocked until the engine is safely stopped.';
      }
    }
  }
  async function chooseProject(path: string) {
    if (busy || status === 'connecting' || status === 'disconnecting') {
      errorMessage = 'Wait for the active connection or turn to finish before switching projects.';
      return;
    }
    if (path !== activeProject) {
      await disconnect();
      if (status !== 'disconnected') return;
      activeProject = path;
      activeThread = '';
      messages = [];
    }
    navOpen = false;
    persist();
  }
  async function connect() {
    if (status === 'connecting' || status === 'disconnecting' || status === 'ready') return;
    if (!selectedProject || !executable.trim()) { errorMessage = 'Provide a Codex executable and choose a project.'; return; }
    if (!bridge) { errorMessage = 'Tauri desktop host unavailable. Browser mode cannot run Codex.'; return; }
    const epoch = ++connectionEpoch;
    fence.beginConnect();
    pending.clear();
    earlyReplies.clear();
    errorMessage = '';
    status = 'connecting';
    try {
      const session = await bridge.connect(executable.trim(), selectedProject.path);
      if (epoch !== connectionEpoch) return;
      // Ready/handshake notifications may precede the invoke result.
      const buffered = fence.connected(session);
      persist();
      for (const event of buffered) onEngine(event);
    } catch {
      if (epoch !== connectionEpoch) return;
      fence.beginDisconnect();
      status = 'error';
      errorMessage = 'Could not start Codex. Check the selected executable and project directory.';
    }
  }
  async function request(method: Parameters<EngineBridge['call']>[0], params: Record<string, unknown>, kind: 'models' | 'create' | 'resume' | 'turn' | 'interrupt') {
    const session = fence.session;
    if (!bridge || session === null) return;
    try {
      const id = await bridge.call(method, params);
      if (fence.session !== session) return; // stale invoke acknowledgement
      pending.set(id, kind);
      const early = earlyReplies.get(id);
      if (early) { earlyReplies.delete(id); onEngine(early); }
    } catch {
      if (fence.session !== session) return;
      busy = false;
      pendingPrompt = '';
      errorMessage = 'The engine rejected this command or disconnected.';
    }
  }
  function parseModels(raw: unknown): string[] {
    if (!raw || typeof raw !== 'object') return [];
    const data = (raw as Record<string, unknown>).data;
    return Array.isArray(data) ? data.map(m => m && typeof m === 'object' ? (m as Record<string, unknown>).model : undefined)
      .filter((name): name is string => typeof name === 'string' && !!name).slice(0, 100) : [];
  }
  function threadId(raw: unknown): string {
    if (!raw || typeof raw !== 'object') return '';
    const data = raw as Record<string, unknown>;
    const thread = data.thread;
    const id = thread && typeof thread === 'object' ? (thread as Record<string, unknown>).id : data.threadId;
    return typeof id === 'string' && id.length <= 256 ? id : '';
  }
  async function dispatchTurn(text: string) {
    if (!activeThread) { errorMessage = 'Thread was not created.'; busy = false; return; }
    const last = messages[messages.length - 1];
    if (!last || last.role !== 'user' || last.text !== text) {
      messages = cappedMessages([...messages, createMessage('user', text, ++counter)]);
    }
    messages = cappedMessages([...messages, createMessage('assistant', '', ++counter)]);
    await request('turn/start', { threadId: activeThread, input: [{ type: 'text', text }], ...(model ? { model } : {}) }, 'turn');
    await tick(); scrollLatest();
  }
  function onEngine(event: EngineEvent) {
    if (!fence.accept(event)) return;
    if (event.type === 'status') {
      status = event.state;
      if (event.state === 'ready') void request('model/list', {}, 'models');
      if (event.state === 'disconnected' || event.state === 'error') {
        busy = false; activeTurn = '';
        if (event.state === 'disconnected') {
          fence.beginDisconnect();
          pending.clear();
          earlyReplies.clear();
        }
      }
      if (event.message) errorMessage = event.message;
      return;
    }
    if (event.type === 'approvalBlocked') { errorMessage = 'Tool permission declined until the NC-07 approval UI is available.'; return; }
    if (event.type === 'reply') {
      const kind = pending.get(event.id);
      pending.delete(event.id);
      if (!kind) {
        if (event.id > 0 && earlyReplies.size < 16) earlyReplies.set(event.id, event);
        return;
      }
      if (!event.ok) {
        errorMessage = 'Codex rejected the ' + kind + ' operation.';
        if (kind === 'turn' || kind === 'create') { busy = false; pendingPrompt = ''; }
        return;
      }
      if (kind === 'models') { models = parseModels(event.result); return; }
      if (kind === 'create') {
        const id = threadId(event.result);
        if (!id) { busy = false; errorMessage = 'Codex did not provide a thread ID.'; return; }
        activeThread = id;
        threads = addThread(threads, { id, project: activeProject, title: pendingPrompt.slice(0, 96), model });
        persist();
        const queued = pendingPrompt;
        pendingPrompt = '';
        void dispatchTurn(queued);
      }
      if (kind === 'resume') {
        const id = threadId(event.result);
        if (id !== activeThread) { errorMessage = 'Resume returned the wrong thread.'; return; }
        messages = restoreThreadMessages(event.result);
        void tick().then(scrollLatest);
      }
      if (kind === 'interrupt') { errorMessage = 'Interrupt requested; awaiting engine acknowledgement.'; }
      return;
    }
    const params = event.params;
    if (event.type === 'turnStarted') {
      const thread = params.threadId;
      if (thread && thread !== activeThread) return;
      const turn = params.turn;
      const id = turn && typeof turn === 'object' ? (turn as Record<string, unknown>).id : params.turnId;
      if (typeof id === 'string') activeTurn = id;
      return;
    }
    if (event.type === 'turnCompleted') {
      if (params.threadId && params.threadId !== activeThread) return;
      busy = false;
      activeTurn = '';
      return;
    }
    if (event.type === 'delta') {
      if (params.threadId && params.threadId !== activeThread) return;
      const delta = params.delta;
      if (typeof delta !== 'string') return;
      const last = messages[messages.length - 1];
      if (!last || last.role !== 'assistant') return;
      try {
        messages = [...messages.slice(0, -1), { ...last, text: appendDelta(last.text, delta) }];
        void tick().then(scrollLatest);
      } catch {
        errorMessage = 'Stream reached the bounded UI limit; disconnect to stop the engine.';
        void disconnect();
      }
    }
  }
  async function newThread() {
    if (busy) { errorMessage = 'Stop the active turn before opening another thread.'; return; }
    activeThread = '';
    messages = [];
    draft = '';
    navOpen = false;
    await tick(); composerEl?.focus();
  }
  async function resumeThread(thread: Thread) {
    if (busy) { errorMessage = 'Stop the active turn before resuming.'; return; }
    if (thread.project !== activeProject) await chooseProject(thread.project);
    activeThread = thread.id;
    messages = [];
    navOpen = false;
    if (status !== 'ready') {
      errorMessage = 'Connect to this project, then resume the selected thread.';
      return;
    }
    await request('thread/resume', { threadId: thread.id, excludeTurns: false }, 'resume');
  }
  async function send() {
    const text = draft.trim();
    if (!text || busy || status !== 'ready') return;
    errorMessage = '';
    busy = true;
    draft = '';
    if (!activeThread) {
      pendingPrompt = text;
      await request('thread/start', { cwd: activeProject, ...(model ? { model } : {}) }, 'create');
    } else {
      await dispatchTurn(text);
    }
  }
  async function stop() {
    if (!busy) return;
    if (!activeTurn) { errorMessage = 'Waiting for the server turn ID before it can be interrupted.'; return; }
    await request('turn/interrupt', { threadId: activeThread, turnId: activeTurn }, 'interrupt');
  }
  function keydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) { event.preventDefault(); void send(); }
  }
  function windowKeydown(event: KeyboardEvent) {
    const action = resolveShortcut(event);
    if (!action) return;
    if (action === 'dismiss') { if (navOpen) { event.preventDefault(); navOpen = false; } return; }
    event.preventDefault();
    if (action === 'search') { navOpen = true; void tick().then(() => searchEl?.focus()); }
    if (action === 'composer') composerEl?.focus();
    if (action === 'new-thread') void newThread();
  }
  onMount(() => {
    projects = restoreProjects(k => localStorage.getItem(k));
    threads = restoreThreads(k => localStorage.getItem(k));
    activeProject = projects[0]?.path ?? '';
    try { executable = localStorage.getItem('ultima-codex-executable-v1') ?? ''; } catch { /* disabled */ }
    let storedTheme = '';
    try { storedTheme = localStorage.getItem('ultima-prototype-theme') ?? ''; } catch { /* disabled */ }
    setTheme(storedTheme === 'light' ? 'light' : 'dark');
    let disposed = false;
    if (desktopAvailable()) {
      void nativeBridge().then(async native => {
        if (disposed) return;
        bridge = native;
        const remove = await native.listen(onEngine);
        if (disposed) remove(); else unlisten = remove;
      }).catch(() => { errorMessage = 'Desktop IPC is unavailable.'; });
    }
    return () => {
      disposed = true;
      unlisten?.();
      void bridge?.disconnect();
    };
  });
</script>

<svelte:window onkeydown={windowKeydown}/>
<div class="shell nc06">
  {#if navOpen}<button class="mobile-scrim" aria-label="Close navigation" onclick={() => navOpen = false}></button>{/if}
  <aside class="sidebar" class:open={navOpen} aria-label="Primary navigation">
    <div class="sidebar-head"><div class="brand"><span class="brand-mark" aria-hidden="true"><span></span><span></span><span></span><span></span></span><span class="brand-name">ultima<span class="brand-period">.</span></span></div><span class="edition">CODEX / LIVE</span></div>
    <button class="new-thread" onclick={() => void newThread()}><span class="new-thread-symbol">+</span> New thread <kbd>Ctrl N</kbd></button>
    <label class="search-box"><span class="sr-only">Search projects and threads</span><input bind:this={searchEl} bind:value={search} placeholder="Find a project or thread" /></label>
    <div class="sidebar-group">
      <div class="group-label"><span>PROJECTS</span><span class="group-count">{projects.length}</span></div>
      {#each shownProjects as project (project.path)}
        <button class="project-item" class:active={activeProject === project.path} onclick={() => void chooseProject(project.path)} title={project.path}><span class="project-symbol">{project.name.slice(0,1).toUpperCase()}</span><span>{project.name}</span></button>
      {/each}
      <div class="nc06-add"><input aria-label="Absolute project folder" placeholder="C:\projects\repo" bind:value={projectInput} onkeydown={e => { if (e.key === 'Enter') addProject(); }} /><button onclick={addProject}>Add</button></div>
    </div>
    <div class="sidebar-group history-links">
      <div class="group-label"><span>RECENT THREADS</span><span class="group-count">{shownThreads.length}</span></div>
      {#each shownThreads as thread (thread.id)}
        <button class="history-link" class:active={thread.id === activeThread} onclick={() => void resumeThread(thread)}><span class="tiny-square" aria-hidden="true"></span><span class="nc06-truncate">{thread.title}</span></button>
      {:else}<p class="empty-search">No saved thread IDs for this project.</p>{/each}
    </div>
    <div class="sidebar-bottom"><span class="mock-indicator"><span class="mock-dot"></span> {status === 'ready' ? 'Codex connected' : 'Local desktop engine'}</span><span class="build-label">NC-06 · AUTH THROUGH CODEX</span></div>
  </aside>
  <main class="main-panel">
    <header class="topbar">
      <div class="topbar-left"><button class="icon-button menu-toggle" aria-label="Open navigation" onclick={() => navOpen = !navOpen}>☰</button><span class="breadcrumb">Workspace</span><span class="slash">/</span><span class="breadcrumb-current">{selectedProject?.name ?? 'Select a project'}</span></div>
      <div class="topbar-actions"><span class="nc06-status" aria-live="polite">{status}</span><button class="icon-button" aria-label="Toggle theme" onclick={() => setTheme(theme === 'dark' ? 'light' : 'dark')}>◐</button></div>
    </header>
    <section class="conversation" aria-labelledby="conversation-title">
      <div class="conversation-head"><p class="eyebrow">CODEX / LOCAL APP SERVER</p><h1 id="conversation-title">{activeThread ? (threads.find(t => t.id === activeThread)?.title ?? 'Codex conversation') : 'Build with clarity'}<span class="heading-period">.</span></h1><p class="conversation-subtitle">{activeThread ? 'Thread resumes through your existing Codex session.' : 'Select a project and connect your Codex installation to begin.'}</p>
        <div class="conversation-meta"><span data-testid="nc06-message-count">{messages.length} messages</span><span class="meta-divider"></span><span>{busy ? 'Streaming' : status === 'ready' ? 'Ready' : 'Not connected'}</span></div>
      </div>
      {#if errorMessage}<p class="nc06-error" role="alert">{errorMessage}<button aria-label="Dismiss error" onclick={() => errorMessage = ''}>×</button></p>{/if}
      {#if status !== 'ready'}
        <div class="nc06-connect" aria-label="Codex connection">
          <label for="nc06-executable">Trusted Codex executable path (selected by you)</label>
          <input id="nc06-executable" placeholder="C:\path\to\codex.exe" bind:value={executable} autocomplete="off"/>
          <p>Only the selected executable is launched via stdio in the selected project. Ultima never collects your Codex credentials.</p>
          <button class="secondary-action" disabled={!selectedProject || status === 'connecting' || status === 'disconnecting' || !executable.trim() || !desktopAvailable()} onclick={() => void connect()}>Connect to Codex</button>
          {#if !desktopAvailable()}<p data-testid="nc06-browser-limit">Browser preview only — launch the Tauri desktop to connect.</p>{/if}
        </div>
      {:else}<div class="nc06-engine"><button class="secondary-action" onclick={() => void disconnect()}>Disconnect engine</button><label for="nc06-model">Model</label><select id="nc06-model" bind:value={model} disabled={busy}><option value="">Engine default</option>{#each models as option}<option value={option}>{option}</option>{/each}</select></div>{/if}
      <div class="conversation-body">
        {#if messages.length === 0}<div class="empty-state"><span class="empty-orbit" aria-hidden="true">✳</span><h2>Start a Codex thread.</h2><p>Recent thread IDs are saved locally, but message content comes from the Codex App Server.</p></div>
        {:else}
          <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
          <div class="feed" bind:this={feedEl} use:measureFeed onscroll={e => scrollTop = e.currentTarget.scrollTop} role="feed" aria-label="Codex conversation" aria-busy={busy} tabindex="0" data-testid="nc06-feed">
            <div class="virtual-canvas" style:height={messages.length * ROW_HEIGHT + 'px'}>
              {#each shownMessages as message, offset (message.id)}
                <article class="message-row" class:user-message={message.role === 'user'} style:top={(range.start + offset) * ROW_HEIGHT + 'px'} aria-posinset={range.start + offset + 1} aria-setsize={messages.length} data-testid="nc06-row">
                  <div class="message-avatar">{message.role === 'user' ? 'Y' : 'U'}</div><div class="message-content"><div class="message-line"><span class="message-author">{message.role === 'user' ? 'You' : 'Codex'}</span></div><p>{message.text || 'Thinking…'}</p></div>
                </article>
              {/each}
            </div>
          </div>
        {/if}
      </div>
    </section>
    <div class="composer-area"><div class="composer-shell" class:busy><label for="composer" class="sr-only">Message to Codex</label><textarea id="composer" bind:this={composerEl} bind:value={draft} onkeydown={keydown} maxlength="8000" rows="2" placeholder="Ask Codex to build something…" disabled={busy || status !== 'ready'}></textarea><div class="composer-bottom"><span class="composer-mode">◇ Codex <span class="mode-separator"></span>{model || 'Default model'}</span>
      {#if busy}<button class="send-button stop-button" aria-label="Interrupt Codex turn" onclick={() => void stop()}><span class="stop-icon"></span></button>{:else}<button class="send-button" aria-label="Send message to Codex" disabled={status !== 'ready' || !draft.trim()} onclick={() => void send()}>↑</button>{/if}
    </div></div><div class="under-composer"><p>Enter to send · Shift+Enter newline · Ctrl/⌘+K search</p><span>LOCAL STDIO · NO BUILT-IN BROWSER</span></div></div>
  </main>
</div>
