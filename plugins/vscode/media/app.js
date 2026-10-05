(() => {
    'use strict';
    const api = acquireVsCodeApi();
    const el = id => document.getElementById(id);
    const session = Date.now().toString(36) + Math.random().toString(36).slice(2);
    const pending = new Map(); let counter = 0; let generation = 0; let targetGeneration = 0; let debounce;
    let reviewToken = null; let reviewing = false; let reviewPending = 0;
    let selected = null; let selectedBinding = null; let selectedReview = null; let current = { file: null, workspaceId: null };
    let offset = 0; let editor = null; let draftTags = []; let lastHits = [];
    let testMode = false;
    const saved = api.getState() || {};
    if (saved.filter) {
        el('libraryScope').value = ['all', 'personal', 'project'].includes(saved.filter.libraryScope) ? saved.filter.libraryScope : 'all';
        el('query').value = typeof saved.filter.query === 'string' ? saved.filter.query : '';
        el('kind').value = saved.filter.kind || '';
        for (const name of ['starred', 'projectOnly']) el(name).checked = saved.filter[name] === true;
    }
    function send(action, payload = {}) {
        const id = session + ':' + (++counter);
        return new Promise((resolve, reject) => { pending.set(id, { resolve, reject }); api.postMessage({ id, action, ...payload }); });
    }
    function filter() { return { query: el('query').value, kind: el('kind').value, tag: el('tag').value,
        starred: el('starred').checked, demo: false, limit: 50, offset }; }
    function stash() {
        api.setState({ filter: { ...filter(), projectOnly: el('projectOnly').checked, libraryScope: el('libraryScope').value }, draft: editor ? { ...editor, scope: el('saveScope').value, bindingKind: el('bindingKind').value, input: formInput() } : null });
    }
    function status(message) { el('status').textContent = message; }
    function error(failure) { status(failure instanceof Error ? failure.message : String(failure)); }
    function node(tag, text, className) { const n = document.createElement(tag); if (text !== undefined) n.textContent = text; if (className) n.className = className; return n; }
    function recordKey(record) { return record ? JSON.stringify([record.scope || 'personal', record.project_root || '', record.id]) : ''; }
    function scopeName(record) { return record.is_demo ? '示例' : record.scope === 'project' ? '项目共享' : '个人'; }
    function reviewLabel(state) { return ({current:'代码未变更',needs_review:'代码已变化，说明待复核',unconfirmed:'尚未确认基线',unavailable:'代码暂不可检查'})[state?.status] || '尚未确认基线'; }
    function attribution(record) {
        return record.scope === 'project' ? '创建者：' + (record.created_by?.display_name || '历史未署名') + ' · 最近修改：' + (record.updated_by?.display_name || '历史未署名') : '';
    }
    function recordName(record) { return scopeName(record) + ' · ' + (record.input.kind === 'document' ? '文档 · ' : record.input.starred ? '★ 片段 · ' : '片段 · ') + record.input.title; }
    async function refresh(reset = false, tags = false, measurement) {
        const started = performance.now();
        if (reset) offset = 0;
        const ticket = ++generation;
        status('正在查找…');
        try {
            const data = await send('search', { filter: filter(), libraryScope: el('libraryScope').value, projectOnly: el('projectOnly').checked, reloadTags: tags });
            if (ticket !== generation) {
                if (measurement) void send('testMeasured', { token: measurement, error: 'Search superseded' });
                return;
            }
            render(data); stash();
            if (measurement) {
                await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
                void send('testMeasured', { token: measurement, elapsedMs: performance.now() - started, total: data.result.total,
                    rows: el('results').children.length, visible: document.visibilityState === 'visible' && !el('library').hidden });
            }
        } catch (failure) {
            if (ticket === generation) error(failure);
            if (measurement) void send('testMeasured', { token: measurement, error: String(failure) });
        }
    }
    function render(data) {
        current = { file: data.file, workspaceId: data.workspaceId };
        const project = data.project;
        el('project-status').hidden = !project?.error && !project?.exists && el('libraryScope').value !== 'project';
        el('project-status').textContent = project?.error ? '项目共享暂不可用：' + project.error + '。请修复 .codemori/shared.json 后刷新；个人资料仍可使用。'
            : project?.exists ? '项目共享资料已加载，修改会写入 .codemori/shared.json；提交 Git 后同事可获取。'
            : '当前项目暂无共享资料；保存时选择“项目共享”即可创建。';
        if (data.tags) {
            el('tag').replaceChildren(new Option('全部标签', ''));
            for (const tag of data.tags) el('tag').append(new Option(tag, tag));
            const effective = data.filter.tag;
            el('tag').value = effective ? data.tags.find(t => t.toLowerCase() === effective.toLowerCase()) || '' : '';
        }
        el('current-file').textContent = current.file && current.workspaceId ? '当前文件 · ' + current.file : '打开本地项目中的文件以查看关联文档';
        el('documents').replaceChildren();
        reviewToken = data.reviewToken || null;
        const pendingReviews = (data.associations || []).filter(e => e.review_state?.status === 'needs_review' && e.review_state.fingerprint);
        reviewPending = pendingReviews.length;
        const modules = pendingReviews.filter(e => e.inherited).length;
        el('reviewCurrent').disabled = reviewing || !reviewToken || pendingReviews.length === 0;
        el('reviewCurrent').textContent = '确认当前文件全部待复核（' + pendingReviews.length + ' 项）';
        el('review-scope').textContent = '点击即确认这些说明仍适用；范围仅当前文件，含 ' + modules + ' 项继承的模块关联（按整个目录确认），不包含无法检查或尚未确认的项目。';
        const associations = data.associations || data.documents.map(record => ({record, binding: {workspace_id: data.workspaceId, path: data.file, document_id:record.id}, review_state:null}));
        for (const entry of associations) {
            const record = entry.record;
            const button = node('button', undefined, 'document-link');
            button.append(node('span', scopeName(record), 'badge'));
            if (record.scope === 'project') button.append(node('span', attribution(record), 'muted'));
            button.append(node('strong', record.input.title, 'document-title'));
            button.append(node('span', (entry.inherited ? '继承自目录：' : '文件关联：') + entry.binding.path + ' · ' + reviewLabel(entry.review_state), 'muted'));
            button.append(node('span', '来源：' + record.input.url, 'document-source'));
            button.append(node('span', record.input.description || '未填写摘要', 'document-summary'));
            button.addEventListener('click', () => choose(record, entry.binding, entry.review_state));
            el('documents').append(button);
        }
        if (!data.documents.length) el('documents').append(node('p', '暂无关联文档', 'muted'));
        const oldKey = recordKey(selected);
        lastHits = data.result.items;
        el('results').replaceChildren();
        for (const hit of lastHits) {
            const row = node('li'); const button = node('button', undefined, 'result'); button.dataset.id = hit.record.id; button.dataset.key = recordKey(hit.record);
            button.setAttribute('aria-selected', 'false'); button.append(node('span', recordName(hit.record), 'result-title'));
            if (hit.record.scope === 'project') button.append(node('span', attribution(hit.record), 'muted'));
            const context = node('span', undefined, 'excerpt');
            for (const span of hit.excerpt) context.append(node(span.highlight ? 'mark' : 'span', span.text));
            button.append(context);
            if (hit.workspace_names.length) button.append(node('span', '项目：' + hit.workspace_names.join(' · '), 'result-project'));
            button.addEventListener('click', () => choose(hit.record));
            button.addEventListener('dblclick', () => selectedAction('open'));
            row.append(button); el('results').append(row);
        }
        const comments = data.comments ?? { items: [], total: 0 };
        el('comments').replaceChildren();
        for (const hit of comments.items) {
            const row = node('li'); const button = node('button', undefined, 'result');
            button.append(node('span', hit.workspace_name + ' · ' + hit.source.path + ':' + hit.source.line, 'result-title'));
            const excerpt = node('span', undefined, 'excerpt');
            for (const span of hit.excerpt) excerpt.append(node(span.highlight ? 'mark' : 'span', span.text));
            button.append(excerpt);
            button.append(node('span', '索引于 ' + new Date(hit.indexed_at).toLocaleString() + ' · 点击跳转', 'muted'));
            button.addEventListener('click', () => send('openComment', { source: hit.source }).catch(error));
            row.append(button); el('comments').append(row);
        }
        const indexed = data.indexStatus;
        el('index-status').textContent = (indexed ? '当前项目本机已索引 ' + indexed.files + ' 个文件 / ' + indexed.comments + ' 条注释。' : '当前项目尚未索引。') +
            '仅扫描已保存文件；修改后请从“更多”增量索引。Java、JS/TS、Python；忽略隐藏、Git 忽略和构建目录。';
        el('count').textContent = data.result.total + ' 条资料 · ' + comments.total + ' 条注释';
        el('empty').hidden = data.result.total !== 0 || comments.total !== 0;
        const scope = [({ all: '个人 + 当前项目共享', personal: '个人资料', project: '项目共享' })[data.libraryScope || el('libraryScope').value], data.filter.workspace_id ? '当前项目' : '全部项目',
            ({ snippet: '代码片段', document: '文档摘要', comment: '源码注释' })[data.filter.kind] || '全部来源'];
        if (data.filter.tag) scope.push('标签：' + data.filter.tag);
        if (data.filter.starred) scope.push('仅个人收藏');
        const emptyMessage = '没有匹配结果。当前范围：' + scope.join(' · ') + '。可清除筛选、保存片段或关联文档。';
        el('empty').textContent = emptyMessage;
        el('previous').disabled = offset === 0; el('next').disabled = offset + 50 >= Math.max(data.result.total, comments.total);
        choose(lastHits.find(hit => recordKey(hit.record) === oldKey)?.record || lastHits[0]?.record || null);
        status(data.result.total || comments.total ? '第 ' + (offset / 50 + 1) + ' 页 · 文档仅检索手填摘要' : emptyMessage);
        if (testMode) void send('testProjectRendered', { version: data.project?.version ?? null });
    }
    function choose(record, binding = null, state = null) {
        selected = record; selectedBinding = binding; selectedReview = state;
        const targetTicket = ++targetGeneration;
        el('openFeishu').hidden = true;
        el('open').textContent = '打开来源 / 原文';
        for (const button of el('results').querySelectorAll('.result')) button.setAttribute('aria-selected', String(button.dataset.key === recordKey(record) && !binding));
        el('preview').hidden = !record;
        if (!record) return;
        const input = record.input; const documentRecord = input.kind === 'document';
        el('preview-title').textContent = input.title;
        el('preview-authors').textContent = attribution(record);
        el('preview-kind').textContent = scopeName(record) + ' · ' + (documentRecord ? '文档摘要' : input.language);
        el('preview-tags').textContent = input.tags.join(' · ');
        el('preview-description').textContent = (documentRecord ? '手填摘要（不包含外部正文）\n' : '') + input.description;
        el('preview-code').textContent = documentRecord ? input.url : input.content;
        el('preview-origin').textContent = input.source ? '保存时来源：' + input.source.path + (input.source.line ? ':' + input.source.line : '') : '';
        el('markdown').hidden = !documentRecord || !(/^file:/i.test(input.url ?? '') || (record.scope === 'project' && (input.url || '').startsWith('./')));
        el('favorite').hidden = documentRecord || record.scope === 'project'; el('favorite').textContent = input.starred ? '取消收藏' : '收藏';
        el('doc-actions').hidden = !documentRecord;
        el('previewSnippet').hidden = documentRecord;
        el('bind').disabled = record.is_demo || !current.file || !current.workspaceId;
        el('unlink').disabled = !binding;
        el('bindModule').disabled = record.is_demo || !current.file || !current.workspaceId;
        el('changes').disabled = !binding; el('review').disabled = !binding || !state?.fingerprint;
        el('preview-review').hidden = !binding;
        el('preview-review').textContent = binding ? (binding.kind === 'module' ? '模块关联：' : '文件关联：') + binding.path + ' · ' + reviewLabel(state) + '（仅检查已保存代码）' + (state?.error ? '\n' + state.error : '') + (binding.review ? '\n上次确认：' + (binding.review.confirmed_by?.display_name || '本机用户') + ' · ' + new Date(binding.review.confirmed_at).toLocaleString() : '') : '';
        if (documentRecord) void send('documentTargets', { recordId: record.id, revision: record.revision, scope: record.scope || 'personal', projectRoot: record.project_root, projectVersion: record.project_version }).then(targets => {
            if (targetTicket !== targetGeneration) return;
            el('openFeishu').hidden = !targets.feishu_applink;
            if (targets.feishu_applink) el('open').textContent = '浏览器打开';
        }).catch(failure => { if (targetTicket === targetGeneration) error(failure); });
    }
    async function selectedAction(action) {
        if (!selected) return;
        const record = selected; const binding = selectedBinding;
        try {
            const result = await send(action, { recordId: record.id, revision: record.revision, binding, fingerprint: selectedReview?.fingerprint, scope: record.scope || 'personal', projectRoot: record.project_root, projectVersion: record.project_version });
            if (action === 'copy') status(record.input.kind === 'snippet' ? '已复制代码快照。' : '已复制文档链接。');
            else if (action === 'openFeishu') status('已请求飞书打开；若未唤起，可点击“浏览器打开”。');
            else if (['favorite', 'delete', 'bind', 'bindModule', 'unlink', 'repairBinding', 'review'].includes(action) && !result?.cancelled) await refresh(action === 'delete', true);
        } catch (failure) { error(failure); }
    }
    function showEditor(value) {
        editor = structuredClone(value); draftTags = [...(value.input.tags || [])];
        const input = value.input; const isDocument = input.kind === 'document';
        el('saveScope').value = value.scope || 'personal';
        el('saveScope').disabled = value.scopeLocked === true;
        el('saveScope').querySelector('[value="project"]').disabled = !value.project || Boolean(value.project.error);
        el('scope-hint').textContent = value.scopeLocked ? '编辑会保留这条资料原来的保存范围。'
            : value.project?.error ? '项目共享暂不可用：' + value.project.error
            : value.project ? '个人资料仅本机保存；项目共享写入仓库文件，由你提交 Git。收藏不会共享。' : '打开本地项目后才可保存项目共享资料。';
        el('editor-title').textContent = value.title;
        el('binding-kind-label').hidden = !value.binding; el('bindingKind').value = value.bindingKind || 'file';
        el('binding-hint').textContent = value.binding ? '关联文件：' + value.binding.path : '';
        for (const name of ['title', 'content', 'language', 'description', 'url']) el(name).value = input[name] || '';
        el('sourcePath').value = input.source?.path || '';
        el('code-fields').hidden = isDocument; el('url-fields').hidden = !isDocument;
        el('source-fields').hidden = !input.source;
        el('title').required = isDocument; el('content').required = !isDocument; el('url').required = isDocument;
        el('description-label').textContent = isDocument ? '摘要（仅搜索本地填写内容）' : '场景描述';
        el('tag-suggestions').replaceChildren();
        for (const tag of value.tags || []) el('tag-suggestions').append(new Option(tag));
        el('tag-input').value = ''; renderTags();
        el('fields').disabled = false; el('editor-error').textContent = '';
        el('library').hidden = true; el('editor').hidden = false; el('title').focus(); stash();
    }
    function renderTags() {
        el('tag-chips').replaceChildren();
        for (const tag of draftTags) {
            const chip = node('span', undefined, 'chip'); chip.append(node('span', tag));
            const remove = node('button', '×'); remove.type = 'button'; remove.setAttribute('aria-label', '移除标签 ' + tag);
            remove.addEventListener('click', () => { draftTags = draftTags.filter(t => t !== tag); renderTags(); stash(); });
            chip.append(remove); el('tag-chips').append(chip);
        }
    }
    function addTag() {
        const tag = el('tag-input').value.trim(); if (tag && !draftTags.includes(tag)) draftTags.push(tag);
        el('tag-input').value = ''; renderTags(); stash();
    }
    function formInput() {
        if (!editor) return null;
        const input = structuredClone(editor.input); const isDocument = input.kind === 'document';
        for (const name of ['title', 'description']) input[name] = el(name).value;
        input.content = isDocument ? '' : el('content').value;
        input.language = isDocument ? '' : el('language').value;
        input.url = isDocument ? el('url').value : null;
        input.tags = [...draftTags];
        if (isDocument) { input.source = null; input.starred = false; }
        else if (input.source) input.source.path = el('sourcePath').value;
        return input;
    }
    el('record-form').addEventListener('submit', async event => {
        event.preventDefault(); if (!editor) return;
        addTag(); const token = editor.token; const input = formInput();
        el('fields').disabled = true; el('editor-error').textContent = '';
        try {
            const record = await send('save', { token, input, scope: el('saveScope').value, bindingKind: el('bindingKind').value });
            if (!editor.scopeLocked) {
                el('query').value = ''; el('kind').value = ''; el('tag').value = '';
                for (const name of ['starred', 'projectOnly']) el(name).checked = false;
            }
            editor = null; el('editor').hidden = true; el('library').hidden = false; selected = record; el('libraryScope').value = record.scope === 'project' ? 'project' : 'personal'; stash();
            await refresh(true, true); status('已保存到' + scopeName(record) + '：' + record.input.title + (record.scope === 'project' ? '；提交 Git 后同事可获取。' : ''));
        } catch (failure) { el('editor-error').textContent = failure instanceof Error ? failure.message : String(failure); el('fields').disabled = false; stash(); }
    });
    el('reviewCurrent').addEventListener('click', async () => {
        if (reviewing || !reviewToken) return;
        reviewing = true; el('reviewCurrent').disabled = true; el('batch-review-result').hidden = false; el('batch-review-result').textContent = '正在确认当前文件的待复核关联…';
        try {
            const result = await send('reviewCurrent', {token:reviewToken});
            await refresh(false, true);
            el('batch-review-result').textContent = result.file + '：已确认 ' + result.confirmed + ' 项。' + (result.errors.length ? '\n未完成：' + result.errors.join('\n') : '');
        } catch (failure) { await refresh(false, true); el('batch-review-result').textContent = failure instanceof Error ? failure.message : String(failure); }
        finally { reviewing = false; el('reviewCurrent').disabled = !reviewToken || reviewPending === 0; }
    });
    el('bindingKind').addEventListener('change', stash);
    el('saveScope').addEventListener('change', stash);
    el('libraryScope').addEventListener('change', () => { if (el('libraryScope').value === 'project') el('starred').checked = false; refresh(true, true); });
    el('cancel').addEventListener('click', async () => {
        try { await send('discard'); editor = null; el('editor').hidden = true; el('library').hidden = false; stash(); }
        catch (failure) { error(failure); }
    });
    el('addTag').addEventListener('click', addTag);
    el('tag-input').addEventListener('keydown', event => { if (event.key === 'Enter') { event.preventDefault(); addTag(); } });
    el('record-form').addEventListener('input', stash);
    el('query').addEventListener('input', () => { clearTimeout(debounce); debounce = setTimeout(() => refresh(true), 100); });
    el('query').addEventListener('keydown', event => { if (event.key === 'Enter') { clearTimeout(debounce); void refresh(true); } });
    el('search').addEventListener('click', () => { clearTimeout(debounce); void refresh(true); });
    el('refresh').addEventListener('click', () => refresh(false, true));
    for (const name of ['kind', 'tag', 'projectOnly', 'starred']) el(name).addEventListener('change', () => refresh(true));
    el('previous').addEventListener('click', () => { offset = Math.max(0, offset - 50); void refresh(); });
    el('next').addEventListener('click', () => { offset += 50; void refresh(); });
    for (const action of ['copy', 'open', 'openFeishu', 'markdown', 'previewSnippet', 'edit', 'favorite', 'delete', 'bind', 'bindModule', 'unlink', 'repairBinding', 'review', 'changes']) el(action).addEventListener('click', () => selectedAction(action));
    for (const action of ['toggleHints', 'repairPaths', 'copyCodeLink', 'openCodeLink', 'identity', 'capture', 'associate', 'indexFile', 'indexWorkspace', 'newSnippet', 'relocate', 'export', 'import']) {
        el(action).addEventListener('click', async () => {
            el('more').open = false;
            try {
                const result = await send(action);
                if (action === 'indexFile' || action === 'indexWorkspace') {
                    await refresh(true);
                    status('注释索引：更新 ' + result.indexed_files + '，未变 ' + result.unchanged_files + '，清理 ' + result.removed_files +
                        '，跳过 ' + result.skipped_files + '，失败 ' + result.failed_files + '。忽略规则排除的文件未计入跳过数。\n' + result.details.join('\n'));
                }
                else if (action === 'import' || action === 'relocate') { if (!result?.cancelled) await refresh(true, true); }
                else if (action === 'export' && result.path) status('已导出备份：' + result.path);
            } catch (failure) { error(failure); }
        });
    }
    el('dismiss').addEventListener('click', async () => { await send('dismissOnboarding'); el('onboarding').hidden = true; });
    el('results').addEventListener('keydown', event => {
        if (event.key === 'Enter') { event.preventDefault(); void selectedAction('open'); return; }
        if (!['ArrowUp', 'ArrowDown'].includes(event.key) || !lastHits.length) return;
        event.preventDefault();
        const currentIndex = lastHits.findIndex(hit => recordKey(hit.record) === recordKey(selected));
        const index = Math.min(lastHits.length - 1, Math.max(0, currentIndex + (event.key === 'ArrowDown' ? 1 : -1)));
        choose(lastHits[index].record); const button = el('results').querySelectorAll('.result')[index]; button.focus(); button.scrollIntoView({ block: 'nearest' });
    });
    window.addEventListener('message', event => {
        const message = event.data;
        if (!message || typeof message !== 'object') return;
        if (message.type === 'reply') {
            const promise = pending.get(message.id); if (!promise) return; pending.delete(message.id);
            if (message.ok) promise.resolve(message.data); else promise.reject(new Error(message.error || '操作失败。'));
        } else if (message.type === 'testSearch' && testMode) {
            editor = null; el('editor').hidden = true; el('library').hidden = false;
            clearTimeout(debounce); el('query').value = message.query; el('kind').value = ''; el('tag').value = '';
            for (const name of ['starred', 'projectOnly']) el(name).checked = false;
            void refresh(true, false, message.token);
        } else if (message.type === 'edit') showEditor(message);
        else if (message.type === 'contextChanged') {
            if (selectedBinding) choose(null);
            reviewToken = null; el('reviewCurrent').disabled = true;
            selectedBinding = null; current = { file: null, workspaceId: null };
            el('documents').replaceChildren(); el('current-file').textContent = '正在读取当前文件…'; el('unlink').disabled = true;
            void refresh();
        }
    });
    send('ready').then(data => {
        testMode = data.testMode === true;
        el('onboarding').hidden = data.onboardingDismissed;
        if (!editor && saved.draft) showEditor(saved.draft);
        if (!editor) el('query').focus();
        return refresh(false, true).then(async () => {
            if (testMode) {
                await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
                return send('testReady');
            }
        });
    }).catch(error);
})();
