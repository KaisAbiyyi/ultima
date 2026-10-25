<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { createMessage, type UIMessage } from './lib/history';
  import { visibleRange } from './lib/virtual';
  import { isTextComposing, resolveShortcut, shouldSendComposer } from './lib/shortcuts';
  import { nativeBridge, desktopAvailable, type EngineBridge, type EngineEvent, type ManagedRuntimeStatus } from './lib/desktop-bridge';
  import { EngineSessionFence } from './lib/engine-session-fence';
  import { FrameDeltas } from './lib/frame-deltas';
  import { readerPage, rowPreview, ROW_PREVIEW_CHARS } from './lib/message-paging';
  import { approvalToolIdentity, canApprove, canApproveInActiveTurn, matchesApprovalEvidence, parseApproval, sameApproval, toolIdentity, trackTool, type ApprovalRequest, type ToolItem } from './lib/tool-events';
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
  let toolItems: ToolItem[] = [];
  let approvals: ApprovalRequest[] = [];
  let reviewedApproval = false;
  let approvalBusy = false;
  let expandedDiffId = '';
  let expandedOutputId = '';
  let status: 'disconnected' | 'connecting' | 'disconnecting' | 'ready' | 'error' = 'disconnected';
  let busy = false;
  let errorMessage = '';
  let search = '';
  let navOpen = false;
  let mobileNav = false;
  let navigationReturnFocus: HTMLElement | null = null;
  let menuButton: HTMLButtonElement;
  let composing = false;
  let lastCompositionEndAt = Number.NEGATIVE_INFINITY;
  let theme: 'dark' | 'light' = 'dark';
  let scrollTop = 0;
  let viewportHeight = 600;
  let followTail = true;
  let streamTargetId = '';
  let readerId = '';
  let readerPosition = 0;
  let feedEl: HTMLDivElement;
  let composerEl: HTMLTextAreaElement;
  let searchEl: HTMLInputElement;
  let bridge: EngineBridge | null = null;
  let unlisten: (() => void) | null = null;
  let runtimeInfo: ManagedRuntimeStatus | null = null;
  let runtimeSource = '';
  let runtimeBusy = false;
  let counter = 0;
  const pending = new Map<number, 'models' | 'create' | 'resume' | 'turn' | 'interrupt'>();
  // IPC notifications can arrive before invoke() resolves; replay only bounded replies.
  const earlyReplies = new Map<number, Extract<EngineEvent, { type: 'reply' }>>();
  const fence = new EngineSessionFence<EngineEvent>();
  const frames = new FrameDeltas(
    callback => requestAnimationFrame(callback),
    frame => cancelAnimationFrame(frame),
    text => {
      const last = messages[messages.length - 1];
      if (!last || last.role !== 'assistant' || last.id !== streamTargetId) return;
      try {
        messages = cappedMessages([...messages.slice(0, -1), {
          ...last, text: appendDelta(last.text, text)
        }]);
        // Follow only while the reader is pinned to the bottom. Never yank
        // keyboard/mouse scroll back to the latest message.
        void tick().then(() => { if (followTail) scrollLatest(); });
      } catch {
        errorMessage = 'Stream reached the bounded UI limit; disconnect to stop the engine.';
        void disconnect();
      }
    }
  );
  function discardStream() {
    frames.clear();
    streamTargetId = '';
  }
  function closeReader() { readerId = ''; readerPosition = 0; }
  let connectionEpoch = 0;
  $: shownProjects = projects.filter(p => p.name.toLowerCase().includes(search.toLowerCase()));
  $: shownThreads = threads.filter(t => t.project === activeProject && t.title.toLowerCase().includes(search.toLowerCase()));
  $: range = visibleRange(messages.length, scrollTop, viewportHeight, ROW_HEIGHT);
  $: shownMessages = messages.slice(range.start, range.end);
  $: readerMessage = readerId ? messages.find(m => m.id === readerId) : undefined;
  $: page = readerMessage ? readerPage(readerMessage.text, readerPosition) : null;
  $: selectedProject = projects.find(p => p.path === activeProject);
  $: currentApproval = approvals[0] ?? null;
  $: currentTool = currentApproval ? toolItems.find(t => matchesApprovalEvidence(currentApproval, t)) : undefined;
  $: currentApprovalKey = currentApproval ? approvalToolIdentity(currentApproval) : '';
  $: canAccept = !!currentApproval &&
    canApproveInActiveTurn(currentApproval, toolItems, activeThread, activeTurn) &&
    (currentApproval.kind !== 'fileChange' || expandedDiffId === currentApprovalKey) &&
    reviewedApproval && !approvalBusy && status === 'ready';

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
  function closeNavigation() {
    navOpen = false;
    const previous = navigationReturnFocus;
    navigationReturnFocus = null;
    void tick().then(() => {
      if (previous?.isConnected) previous.focus();
      else if (mobileNav) menuButton?.focus();
    });
  }
  function openSearch() {
    if (!navOpen && document.activeElement instanceof HTMLElement) navigationReturnFocus = document.activeElement;
    navOpen = true;
    void tick().then(() => searchEl?.focus());
  }
  function toggleNavigation() {
    if (navOpen) closeNavigation();
    else { navigationReturnFocus = menuButton; navOpen = true; void tick().then(() => searchEl?.focus()); }
  }
  function compositionEnd() {
    composing = false;
    lastCompositionEndAt = performance.now();
  }
  function commitInProgress() {
    return composing || performance.now() - lastCompositionEndAt < 120;
  }
  function scrollLatest() {
    if (!feedEl) return;
    feedEl.scrollTop = Math.max(0, feedEl.scrollHeight - feedEl.clientHeight);
    scrollTop = feedEl.scrollTop;
    followTail = true;
  }
  function onFeedScroll(node: HTMLDivElement) {
    scrollTop = node.scrollTop;
    followTail = node.scrollHeight - node.scrollTop - node.clientHeight <= 128;
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
    discardStream();
    closeReader();
    pending.clear();
    earlyReplies.clear();
    approvals = [];
    toolItems = [];
    reviewedApproval = false;
    expandedDiffId = '';
    expandedOutputId = '';
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
      discardStream();
      closeReader();
      messages = [];
      followTail = true;
    }
    if (navOpen) closeNavigation();
    persist();
  }
  async function refreshRuntime() {
    if (!bridge) return;
    try { runtimeInfo = await bridge.runtimeStatus(); }
    catch { runtimeInfo = null; }
  }
  async function runtimeAction(operation: 'stage' | 'activate' | 'system' | 'rollback') {
    if (!bridge || runtimeBusy || status === 'connecting' || status === 'disconnecting' || status === 'ready') {
      errorMessage = 'Disconnect the active Codex session before changing runtimes.';
      return;
    }
    runtimeBusy = true;
    errorMessage = '';
    try {
      if (operation === 'stage') await bridge.runtimeStage(runtimeSource.trim());
      if (operation === 'activate') {
        if (!selectedProject) throw Error('Select project');
        await bridge.runtimeActivate(selectedProject.path);
      }
      if (operation === 'system') await bridge.runtimeSystem();
      if (operation === 'rollback') await bridge.runtimeRollback();
      await refreshRuntime();
    } catch {
      errorMessage = 'Managed runtime operation rejected. Verify official asset, checksum and disconnect state.';
    } finally { runtimeBusy = false; }
  }
  async function connect() {
    if (status === 'connecting' || status === 'disconnecting' || status === 'ready') return;
    if (!selectedProject || !executable.trim()) { errorMessage = 'Provide a Codex executable and choose a project.'; return; }
    if (!bridge) { errorMessage = 'Tauri desktop host unavailable. Browser mode cannot run Codex.'; return; }
    const epoch = ++connectionEpoch;
    fence.beginConnect();
    discardStream();
    closeReader();
    pending.clear();
    earlyReplies.clear();
    approvals = [];
    toolItems = [];
    reviewedApproval = false;
    expandedDiffId = '';
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
      if (pending.size >= 32) {
        errorMessage = 'Too many unanswered engine requests; disconnecting to limit retained state.';
        void disconnect();
        return;
      }
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
    discardStream();
    followTail = true;
    if (!activeThread) { errorMessage = 'Thread was not created.'; busy = false; return; }
    const last = messages[messages.length - 1];
    if (!last || last.role !== 'user' || last.text !== text) {
      messages = cappedMessages([...messages, createMessage('user', text, ++counter)]);
    }
    messages = cappedMessages([...messages, createMessage('assistant', '', ++counter)]);
    await request('turn/start', { threadId: activeThread, input: [{ type: 'text', text }], ...(model ? { model } : {}) }, 'turn');
    await tick(); scrollLatest();
  }
  async function decideApproval(decision: 'accept' | 'decline') {
    const current = approvals[0];
    const session = fence.session;
    if (!bridge || !current || session === null || approvalBusy) return;
    if (decision === 'accept' && !canAccept) return;
    // Remove BEFORE IPC: if an ACK is lost, never offer a dangerous duplicate retry.
    approvalBusy = true;
    approvals = approvals.slice(1);
    reviewedApproval = false;
    expandedDiffId = '';
    try {
      await bridge.approval(session, current, decision);
    } catch {
      errorMessage = 'The permission decision could not be confirmed. Do not retry; disconnect the engine before continuing.';
    } finally {
      approvalBusy = false;
    }
  }
  function denyWithoutPrompt(request: ApprovalRequest, session: number) {
    // Unexpected thread or bounded queue overflow must fail closed.
    void bridge?.approval(session, request, 'decline').catch(() => {
      errorMessage = 'Unable to safely decline tool permission. Engine shutdown requested.';
      void disconnect();
    });
  }
  function onEngine(event: EngineEvent) {
    if (!fence.accept(event)) return;
    if (event.type === 'status') {
      status = event.state;
      if (event.state === 'ready') void request('model/list', {}, 'models');
      if (event.state === 'disconnected' || event.state === 'error') {
        busy = false; activeTurn = '';
        discardStream();
        approvals = []; toolItems = []; reviewedApproval = false;
        if (event.state === 'disconnected') {
          fence.beginDisconnect();
          pending.clear();
          earlyReplies.clear();
        }
      }
      if (event.message) errorMessage = event.message;
      return;
    }
    if (event.type === 'approvalPending') {
      const request = parseApproval(event.request);
      if (!request) {
        errorMessage = 'Invalid tool approval payload. Disconnect to deny safely.';
        void disconnect();
        return;
      }
      if (status !== 'ready' || request.threadId !== activeThread || request.turnId !== activeTurn ||
          approvals.length >= 16) {
        denyWithoutPrompt(request, event.session);
        return;
      }
      if (!approvals.some(a => sameApproval(a, request))) approvals = [...approvals, request];
      return;
    }
    if (event.type === 'requestResolved') {
      const id = event.params.requestId ?? event.params.serverRequestId ?? event.params.id;
      if (typeof id === 'string' || typeof id === 'number') {
        const previous = approvals[0];
        approvals = approvals.filter(a => a.id !== id);
        if (previous && previous.id === id) {
          reviewedApproval = false;
          expandedDiffId = '';
        }
      }
      return;
    }
    if (event.type === 'itemStarted' || event.type === 'itemCompleted' || event.type === 'commandOutput') {
      // Missing provenance is NOT proof. Never combine same itemId from another turn.
      if (event.params.threadId !== activeThread || event.params.turnId !== activeTurn) return;
      const item = event.params.item;
      const itemId = typeof item === 'object' && item && 'id' in item ? item.id : event.params.itemId;
      if (currentApproval && event.params.threadId === currentApproval.threadId &&
          event.params.turnId === currentApproval.turnId && itemId === currentApproval.itemId) {
        reviewedApproval = false;
        expandedDiffId = '';
      }
      toolItems = trackTool(toolItems, event.type, event.params);
      return;
    }
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
        discardStream();
        messages = restoreThreadMessages(event.result);
        followTail = true;
        void tick().then(scrollLatest);
      }
      if (kind === 'interrupt') { errorMessage = 'Interrupt requested; awaiting engine acknowledgement.'; }
      return;
    }
    const params = event.params;
    if (event.type === 'turnStarted') {
      frames.drain();
      streamTargetId = '';
      const thread = params.threadId;
      if (thread && thread !== activeThread) return;
      const turn = params.turn;
      const id = turn && typeof turn === 'object' ? (turn as Record<string, unknown>).id : params.turnId;
      if (typeof id === 'string') activeTurn = id;
      return;
    }
    if (event.type === 'turnCompleted') {
      if (params.threadId && params.threadId !== activeThread) return;
      if (params.turnId && activeTurn && params.turnId !== activeTurn) return;
      frames.drain();
      streamTargetId = '';
      busy = false;
      activeTurn = '';
      // A completed turn must not leave an actionable stale approval card.
      for (const approval of approvals) denyWithoutPrompt(approval, event.session);
      approvals = []; reviewedApproval = false;
      return;
    }
    if (event.type === 'delta') {
      if (params.threadId && params.threadId !== activeThread) return;
      if (params.turnId && activeTurn && params.turnId !== activeTurn) return;
      const delta = params.delta;
      if (typeof delta !== 'string') return;
      const last = messages[messages.length - 1];
      if (!last || last.role !== 'assistant') return;
      if (streamTargetId && streamTargetId !== last.id) frames.clear();
      streamTargetId = last.id;
      if (!frames.push(delta)) {
        errorMessage = 'Codex sent more stream data than one paint frame can safely retain.';
        void disconnect(); // do not silently lose agent output
      }
    }
  }
  async function newThread() {
    if (busy) { errorMessage = 'Stop the active turn before opening another thread.'; return; }
    activeThread = '';
    discardStream();
    closeReader();
    messages = [];
    followTail = true;
    toolItems = [];
    approvals = [];
    reviewedApproval = false;
    draft = '';
    navOpen = false;
    await tick(); composerEl?.focus();
  }
  async function resumeThread(thread: Thread) {
    if (busy) { errorMessage = 'Stop the active turn before resuming.'; return; }
    if (thread.project !== activeProject) await chooseProject(thread.project);
    activeThread = thread.id;
    discardStream();
    closeReader();
    messages = [];
    followTail = true;
    toolItems = [];
    approvals = [];
    reviewedApproval = false;
    if (navOpen) closeNavigation();
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
    if (shouldSendComposer(event, commitInProgress())) { event.preventDefault(); void send(); }
  }
  function windowKeydown(event: KeyboardEvent) {
    if (event.defaultPrevented) return;
    const action = resolveShortcut(event, commitInProgress());
    if (!action) return;
    if (action === 'dismiss') { if (navOpen) { event.preventDefault(); closeNavigation(); } return; }
    event.preventDefault();
    if (action === 'search') openSearch();
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
    const navMedia = window.matchMedia('(max-width: 640px)');
    const updateNavMedia = () => { mobileNav = navMedia.matches; if (!mobileNav && navOpen) closeNavigation(); };
    updateNavMedia();
    navMedia.addEventListener('change', updateNavMedia);
    let disposed = false;
    if (desktopAvailable()) {
      void nativeBridge().then(async native => {
        if (disposed) return;
        bridge = native;
        void refreshRuntime();
        const remove = await native.listen(onEngine);
        if (disposed) remove(); else unlisten = remove;
      }).catch(() => { errorMessage = 'Desktop IPC is unavailable.'; });
    }
    return () => {
      disposed = true;
      navMedia.removeEventListener('change', updateNavMedia);
      connectionEpoch++
      fence.beginDisconnect();
      discardStream();
      pending.clear();
      earlyReplies.clear();
      unlisten?.();
      void bridge?.disconnect();
    };
  });
</script>

<svelte:window onkeydown={windowKeydown}/>
<div class="shell nc06">
  {#if navOpen && mobileNav}<button class="mobile-scrim" aria-label="Close navigation" onclick={closeNavigation}></button>{/if}
  <aside id="nc09-sidebar" class="sidebar" class:open={navOpen} aria-label="Primary navigation" inert={mobileNav && !navOpen}>
    <div class="sidebar-head"><div class="brand"><span class="brand-mark" aria-hidden="true"><span></span><span></span><span></span><span></span></span><span class="brand-name">ultima<span class="brand-period">.</span></span></div><span class="edition">CODEX / LIVE</span></div>
    <button class="new-thread" aria-keyshortcuts="Control+N Meta+N" onclick={() => void newThread()}><span class="new-thread-symbol">+</span> New thread <kbd>Ctrl N</kbd></button>
    <label class="search-box"><span class="sr-only">Search projects and threads</span><input type="search" aria-label="Search projects and threads" aria-keyshortcuts="Control+K Meta+K" autocomplete="off" bind:this={searchEl} bind:value={search} placeholder="Find a project or thread" /></label>
    <div class="sidebar-group">
      <div class="group-label"><span>PROJECTS</span><span class="group-count">{projects.length}</span></div>
      {#each shownProjects as project (project.path)}
        <button class="project-item" class:active={activeProject === project.path} aria-current={activeProject === project.path ? 'page' : undefined} onclick={() => void chooseProject(project.path)} title={project.path}><span class="project-symbol">{project.name.slice(0,1).toUpperCase()}</span><span>{project.name}</span></button>
      {/each}
      <div class="nc06-add"><input aria-label="Absolute project folder" placeholder="C:\projects\repo" bind:value={projectInput} onkeydown={e => { if (e.key === 'Enter' && !isTextComposing(e, commitInProgress())) { e.preventDefault(); addProject(); } }} oncompositionstart={() => composing = true} oncompositionend={compositionEnd} /><button aria-label="Add project" onclick={addProject}>Add</button></div>
    </div>
    <div class="sidebar-group history-links">
      <div class="group-label"><span>RECENT THREADS</span><span class="group-count">{shownThreads.length}</span></div>
      {#each shownThreads as thread (thread.id)}
        <button class="history-link" class:active={thread.id === activeThread} aria-current={thread.id === activeThread ? 'page' : undefined} onclick={() => void resumeThread(thread)}><span class="tiny-square" aria-hidden="true"></span><span class="nc06-truncate">{thread.title}</span></button>
      {:else}<p class="empty-search">No saved thread IDs for this project.</p>{/each}
    </div>
    <div class="sidebar-bottom"><span class="mock-indicator"><span class="mock-dot"></span> {status === 'ready' ? 'Codex connected' : 'Local desktop engine'}</span><span class="build-label">NC-06 · AUTH THROUGH CODEX</span></div>
  </aside>
  <main class="main-panel">
    <header class="topbar">
      <div class="topbar-left"><button bind:this={menuButton} class="icon-button menu-toggle" aria-label="Open navigation" aria-controls="nc09-sidebar" aria-expanded={navOpen} onclick={toggleNavigation}>☰</button><span class="breadcrumb">Workspace</span><span class="slash">/</span><span class="breadcrumb-current">{selectedProject?.name ?? 'Select a project'}</span></div>
      <div class="topbar-actions"><span class="nc06-status" aria-live="polite">{status}</span><button class="icon-button" aria-label="Toggle theme" aria-pressed={theme === 'light'} onclick={() => setTheme(theme === 'dark' ? 'light' : 'dark')}>◐</button></div>
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
          {#if desktopAvailable()}
            <details aria-label="Optional managed Codex runtime">
              <summary>Optional verified Codex runtime (system default)</summary>
              <p data-testid="nc12-runtime-state">Selection: {runtimeInfo?.mode ?? 'unknown'} {runtimeInfo?.version ?? ''}</p>
              {#if runtimeInfo?.managedSupported}
                <p>Only pinned Codex {runtimeInfo.pinnedVersion} is qualified. No automatic downloads or changes to global Codex.</p>
                {#if runtimeInfo.assetUrl}
                  <a href={runtimeInfo.assetUrl} target="_blank" rel="noopener noreferrer">Official upstream x64 Windows asset</a>
                {/if}
                <label for="nc12-source">Previously downloaded official executable (absolute path)</label>
                <input id="nc12-source" bind:value={runtimeSource} placeholder="C:\\path\\to\\codex-x86_64-pc-windows-msvc.exe" autocomplete="off"/>
                <button class="secondary-action" disabled={runtimeBusy || !runtimeSource.trim() || status === 'connecting' || status === 'disconnecting'} onclick={() => void runtimeAction('stage')}>Verify & stage</button>
                <button class="secondary-action" disabled={runtimeBusy || !selectedProject || status === 'connecting' || status === 'disconnecting'} onclick={() => void runtimeAction('activate')}>Healthcheck & select managed</button>
                <button class="secondary-action" disabled={runtimeBusy || status === 'connecting' || status === 'disconnecting'} onclick={() => void runtimeAction('rollback')}>Rollback selection</button>
              {:else}
                <p>Managed runtime currently supported only for Windows x64. System Codex remains available.</p>
              {/if}
              <button class="secondary-action" disabled={runtimeBusy || status === 'connecting' || status === 'disconnecting'} onclick={() => void runtimeAction('system')}>Use system Codex</button>
            </details>
          {/if}
          {#if !desktopAvailable()}<p data-testid="nc06-browser-limit">Browser preview only — launch the Tauri desktop to connect.</p>{/if}
        </div>
      {:else}<div class="nc06-engine"><button class="secondary-action" onclick={() => void disconnect()}>Disconnect engine</button><label for="nc06-model">Model</label><select id="nc06-model" bind:value={model} disabled={busy}><option value="">Engine default</option>{#each models as option}<option value={option}>{option}</option>{/each}</select></div>{/if}
      <div class="conversation-body">
        {#if messages.length === 0}<div class="empty-state"><span class="empty-orbit" aria-hidden="true">✳</span><h2>Start a Codex thread.</h2><p>Recent thread IDs are saved locally, but message content comes from the Codex App Server.</p></div>
        {:else}
          <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
          <div class="feed" bind:this={feedEl} use:measureFeed onscroll={e => onFeedScroll(e.currentTarget)} role="feed" aria-label="Codex conversation" aria-busy={busy} tabindex="0" data-testid="nc06-feed">
            <div class="virtual-canvas" style:height={messages.length * ROW_HEIGHT + 'px'}>
              {#each shownMessages as message, offset (message.id)}
                <article class="message-row" class:user-message={message.role === 'user'} style:top={(range.start + offset) * ROW_HEIGHT + 'px'} aria-posinset={range.start + offset + 1} aria-setsize={messages.length} data-testid="nc06-row">
                  <div class="message-avatar">{message.role === 'user' ? 'Y' : 'U'}</div><div class="message-content"><div class="message-line"><span class="message-author">{message.role === 'user' ? 'You' : 'Codex'}</span>{#if message.text.length > ROW_PREVIEW_CHARS}<button class="nc08-read" onclick={() => { readerId = message.id; readerPosition = 0; }}>Read message</button>{/if}</div><p>{rowPreview(message.text) || 'Thinking…'}</p></div>
                </article>
              {/each}
            </div>
          </div>
        {/if}
        {#if readerMessage && page}
          <section class="nc08-reader" aria-label="Message reader" data-testid="nc08-reader">
            <div class="nc08-reader-head">
              <strong>{readerMessage.role === 'user' ? 'You' : 'Codex'} · Message detail</strong>
              <button class="secondary-action" aria-label="Close message reader" onclick={closeReader}>Close</button>
            </div>
            <p>Page {page.page + 1} of {page.pages} · text rendered on demand</p>
            <pre>{page.text}</pre>
            <div class="nc08-reader-controls">
              <button class="secondary-action" disabled={!page.hasPrevious} onclick={() => readerPosition--}>Previous</button>
              <button class="secondary-action" disabled={!page.hasNext} onclick={() => readerPosition++}>Next</button>
            </div>
          </section>
        {/if}
      </div>
      {#if currentApproval}
        <section class="nc07-approval" aria-labelledby="nc09-approval-heading" role="dialog" aria-modal="false" data-testid="nc07-approval">
          <p class="eyebrow">PERMISSION REQUIRED · ONE REQUEST ONLY</p>
          <h2 id="nc09-approval-heading">{currentApproval.kind === 'command' ? 'Execute a command?' : 'Apply file changes?'}</h2>
          <p>Review the exact command or change preview. Permission defaults to decline; this does not grant future requests.</p>
          {#if currentTool && currentApproval.kind === 'command' && currentTool.kind === 'command'}
            <pre class="nc07-evidence">{currentTool.command}</pre>
          {:else if currentTool && currentApproval.kind === 'fileChange' && currentTool.kind === 'fileChange'}
            <p>{currentTool.paths.join(', ') || 'Paths not available'}</p>
            <button class="secondary-action" aria-expanded={expandedDiffId === currentApprovalKey} onclick={() => expandedDiffId = expandedDiffId === currentApprovalKey ? '' : currentApprovalKey}>Review file diff</button>
            {#if expandedDiffId === currentApprovalKey}<pre class="nc07-evidence nc09-diff" tabindex="0" role="region" aria-label="Complete proposed file diff">{currentTool.diff || 'No diff provided by server.'}</pre>{/if}
          {:else}
            <p class="nc07-warning">The App Server has not provided a verifiable command or diff preview. Accept is unavailable.</p>
          {/if}
          {#if currentTool?.truncated}<p class="nc07-warning">Preview was truncated. Decline this request unless the full action can be reviewed.</p>{/if}
          <label class="nc07-review"><input type="checkbox" bind:checked={reviewedApproval} disabled={!canApprove(currentApproval, toolItems) || (currentApproval.kind === 'fileChange' && expandedDiffId !== currentApprovalKey)} /> I reviewed this specific action</label>
          <div class="nc07-actions">
            <button class="secondary-action" disabled={approvalBusy} onclick={() => void decideApproval('decline')}>Decline</button>
            <button class="secondary-action" disabled={!canAccept} onclick={() => void decideApproval('accept')}>Allow once</button>
          </div>
          <p class="nc07-secondary">{approvals.length} pending · session {fence.session} · item {currentApproval.itemId}</p>
        </section>
      {/if}
      {#if toolItems.length}
        <section class="nc07-tools" aria-label="Recent tool activity" data-testid="nc07-tools">
          <h2>Recent tools <span>{toolItems.length} / 16</span></h2>
          {#each [...toolItems].reverse() as item (toolIdentity(item))}
            <div class="nc07-tool-row"><span>{item.kind === 'command' ? 'Command' : item.kind === 'fileChange' ? 'File changes' : 'Tool'} · {item.state}</span>
              <span>{item.kind === 'command' ? item.command.slice(0, 100) : item.paths.join(', ').slice(0, 100)}</span>
              {#if item.output}<button class="secondary-action" aria-expanded={expandedOutputId === toolIdentity(item)} onclick={() => expandedOutputId = expandedOutputId === toolIdentity(item) ? '' : toolIdentity(item)}>{expandedOutputId === toolIdentity(item) ? 'Hide output' : 'Show output'}</button>{/if}
              {#if item.diff}<button class="secondary-action" aria-expanded={expandedDiffId === toolIdentity(item)} onclick={() => expandedDiffId = expandedDiffId === toolIdentity(item) ? '' : toolIdentity(item)}>{expandedDiffId === toolIdentity(item) ? 'Hide diff' : 'Show diff'}</button>{/if}
              {#if item.truncated}<small>Truncated</small>{/if}
              {#if expandedOutputId === toolIdentity(item)}<pre class="nc07-evidence" tabindex="0" role="region" aria-label="Command output">{item.output}</pre>{/if}
              {#if expandedDiffId === toolIdentity(item)}<pre class="nc07-evidence nc09-diff" tabindex="0" role="region" aria-label="Tool file diff">{item.diff}</pre>{/if}
            </div>
          {/each}
        </section>
      {/if}
    </section>
    <div class="composer-area"><div class="composer-shell" class:busy><label for="composer" class="sr-only">Message to Codex</label><textarea id="composer" bind:this={composerEl} bind:value={draft} onkeydown={keydown} oncompositionstart={() => composing = true} oncompositionend={compositionEnd} aria-keyshortcuts="Control+L Meta+L" maxlength="8000" rows="2" placeholder="Ask Codex to build something…" disabled={busy || status !== 'ready'}></textarea><div class="composer-bottom"><span class="composer-mode">◇ Codex <span class="mode-separator"></span>{model || 'Default model'}</span>
      {#if busy}<button class="send-button stop-button" aria-label="Interrupt Codex turn" onclick={() => void stop()}><span class="stop-icon"></span></button>{:else}<button class="send-button" aria-label="Send message to Codex" disabled={status !== 'ready' || !draft.trim()} onclick={() => void send()}>↑</button>{/if}
    </div></div><div class="under-composer"><p>Enter to send · Shift+Enter newline · Ctrl/⌘+K search · Ctrl/⌘+L composer</p><span>LOCAL STDIO · NO BUILT-IN BROWSER</span></div></div>
  </main>
</div>
