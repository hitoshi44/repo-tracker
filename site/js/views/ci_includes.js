import { loadAndBuildRows } from '../ci_includes.js';
import { html, escapeHtml, raw } from '../render.js';

export async function render() {
  const rows = await loadAndBuildRows();
  if (rows.length === 0) {
    return html`
      <section>
        <h2>CI 呼び出し</h2>
        <p>該当 include なし</p>
      </section>
    `;
  }

  // 表に出てくる type の一覧 (動的) を集めて select の選択肢に
  const types = Array.from(new Set(rows.map(r => r.type))).sort();
  const typeOpts = ['<option value="all">すべての type</option>']
    .concat(types.map(t => `<option value="${escapeHtml(t)}">${escapeHtml(t)}</option>`))
    .join('');

  const tbody = rows.map(r => html`
    <tr data-repo-path="${r.repo_path}" data-path="${r.path}" data-type="${r.type}" data-ref="${r.ref}">
      <td><a href="#/repo/${r.repo_id}">${r.repo_path}</a></td>
      <td class="path"><a href="#/file/${r.repo_id}/${encodeURIComponent(r.path)}">${r.path}</a></td>
      <td>${r.type}</td>
      <td>${r.ref}</td>
    </tr>
  `).join('');

  const total = rows.length;
  const html_ = `
    <section>
      <h2>CI 呼び出し</h2>
      <form id="ci-filter" class="filter-form" onsubmit="return false;">
        <input type="text" name="q" placeholder="フィルタ (repo / path / 参照先)" autofocus>
        <select name="type">${typeOpts}</select>
        <span class="filter-count"><span id="ci-filter-count">${total}</span> / ${total}</span>
      </form>
      <table id="ci-table">
        <thead><tr><th>repo</th><th>file</th><th>type</th><th>参照先</th></tr></thead>
        <tbody>${tbody}</tbody>
      </table>
    </section>
  `;
  return { html: html_, after: () => bindFilter(total) };
}

// 純関数: 1 行が表示対象かを判定 (テスト対象)
export function matches(row, query, type) {
  if (type && type !== 'all' && row.type !== type) return false;
  if (!query) return true;
  const q = query.toLowerCase();
  return ['repo_path', 'path', 'type', 'ref']
    .some(k => (row[k] || '').toLowerCase().includes(q));
}

function bindFilter(total) {
  const form = document.getElementById('ci-filter');
  const tbody = document.querySelector('#ci-table tbody');
  if (!form || !tbody) return;
  const trs = Array.from(tbody.querySelectorAll('tr'));
  const countEl = document.getElementById('ci-filter-count');

  function apply() {
    const q = form.querySelector('input[name=q]').value;
    const t = form.querySelector('select[name=type]').value;
    let visible = 0;
    for (const tr of trs) {
      const row = {
        repo_path: tr.dataset.repoPath,
        path: tr.dataset.path,
        type: tr.dataset.type,
        ref: tr.dataset.ref,
      };
      const show = matches(row, q, t);
      tr.style.display = show ? '' : 'none';
      if (show) visible++;
    }
    countEl.textContent = String(visible);
  }

  form.addEventListener('input', apply);
  apply();
}
