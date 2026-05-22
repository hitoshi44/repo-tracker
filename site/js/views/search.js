import { loadRepos, loadCiRaws, loadPkgRaws, loadPomRaws, loadDockerRaws } from '../store.js';
import { grep, highlightPositions } from '../grep.js';
import { html, escapeHtml, raw } from '../render.js';
import { buildQuery } from '../router.js';

const GREP_KINDS = [
  { id: 'ci',     label: '.gitlab-ci.yml' },
  { id: 'pkg',    label: 'package.json' },
  { id: 'pom',    label: 'pom.xml' },
  { id: 'docker', label: 'Dockerfile' },
];

export async function render(_params, query) {
  const q             = query.q || '';
  const enabledKinds  = (query.kinds || 'ci,pkg,pom,docker').split(',').filter(Boolean);
  const caseSensitive = query.cs === '1';

  const form = renderForm({ q, enabledKinds, caseSensitive });

  let results;
  if (!q) {
    results = `<p class="stub">クエリを入力してください。</p>`;
  } else {
    results = await renderGrepResults(q, enabledKinds, caseSensitive);
  }

  return { html: `<h2>横断検索</h2>` + form + results, after: bindForm };
}

function renderForm({ q, enabledKinds, caseSensitive }) {
  const grepKinds = GREP_KINDS.map(k => html`
    <label><input type="checkbox" name="kinds" value="${k.id}"
      ${raw(enabledKinds.includes(k.id) ? 'checked' : '')}> ${k.label}</label>
  `).join('');

  return html`
    <form class="search-form" id="search-form">
      <input type="text" name="q" value="${q}" placeholder="検索クエリ" autofocus>
      <button type="submit">検索</button>
      <fieldset><legend>対象</legend>${raw(grepKinds)}</fieldset>
      <label class="cs"><input type="checkbox" name="cs" ${raw(caseSensitive ? 'checked' : '')}> case sensitive</label>
    </form>
  `;
}

async function renderGrepResults(q, enabledKinds, caseSensitive) {
  const sources = [];
  if (enabledKinds.includes('ci'))     sources.push({ kind: 'ci',     entries: await loadCiRaws() });
  if (enabledKinds.includes('pkg'))    sources.push({ kind: 'pkg',    entries: await loadPkgRaws() });
  if (enabledKinds.includes('pom'))    sources.push({ kind: 'pom',    entries: await loadPomRaws() });
  if (enabledKinds.includes('docker')) sources.push({ kind: 'docker', entries: await loadDockerRaws() });

  const hits = grep(sources, q, { caseSensitive });
  if (hits.length === 0) return `<p>マッチなし</p>`;

  const repos = (await loadRepos()).repos || [];
  const repoPathById = new Map(repos.map(r => [r.id, r.path]));

  const items = hits.map(h => {
    const positions = highlightPositions(h.line, q, caseSensitive);
    const lineHtml = highlightLine(h.line, positions);
    const repoLabel = repoPathById.get(h.repo_id) || `repo ${h.repo_id}`;
    return html`
      <div class="match">
        <div class="match-header">
          <span class="kind">${h.kind}</span> &middot;
          <a href="#/file/${h.repo_id}/${encodeURIComponent(h.path)}">${repoLabel} / <span class="path">${h.path}</span></a>
          : line ${h.line_no}
        </div>
        <pre>${raw(lineHtml)}</pre>
      </div>`;
  }).join('');

  return `<p>${hits.length} match(es)</p>` + items;
}

function highlightLine(line, positions) {
  if (positions.length === 0) return escapeHtml(line);
  let out = '';
  let cursor = 0;
  for (const [s, e] of positions) {
    out += escapeHtml(line.slice(cursor, s));
    out += '<mark>' + escapeHtml(line.slice(s, e)) + '</mark>';
    cursor = e;
  }
  out += escapeHtml(line.slice(cursor));
  return out;
}

// フォーム submit を hash に変換するハンドラ。
// router の after フック経由で、innerHTML 反映直後に呼ばれる。
function bindForm() {
  const form = document.getElementById('search-form');
  if (!form) return;
  form.addEventListener('submit', e => {
    e.preventDefault();
    const fd = new FormData(form);
    const params = {
      q: fd.get('q') || '',
      kinds: fd.getAll('kinds').join(','),
      cs: fd.get('cs') ? '1' : '',
    };
    location.hash = '#/search?' + buildQuery(params);
  });
}

export { highlightLine };
