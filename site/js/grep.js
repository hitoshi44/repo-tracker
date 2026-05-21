// 純関数: raw バンドル群を文字列クエリで grep する。
//
// 戻り値はマッチ単位の配列。1 ファイル内に複数行ヒットすれば複数エントリ。
// CI / pkg / pom など複数ソースを受け取れるよう、入力は { kind, entries } の配列。

export function grep(sources, query, opts = {}) {
  const { caseSensitive = false, maxPerFile = 50 } = opts;
  if (!query) return [];

  const needle = caseSensitive ? query : query.toLowerCase();
  const results = [];

  for (const src of sources) {
    for (const entry of src.entries) {
      const hay = caseSensitive ? entry.raw : entry.raw.toLowerCase();
      const lines = entry.raw.split('\n');
      const hayLines = caseSensitive ? lines : lines.map(l => l.toLowerCase());
      let hits = 0;
      for (let i = 0; i < hayLines.length && hits < maxPerFile; i++) {
        const col = hayLines[i].indexOf(needle);
        if (col === -1) continue;
        results.push({
          kind: src.kind,
          repo_id: entry.repo_id,
          path: entry.path,
          line_no: i + 1,
          line: lines[i],
          col,
          length: query.length,
        });
        hits++;
      }
      // hay 全体一回もマッチしなければ全行ループも避けたいが、indexOf 一発の方が
      // 大半のファイルで cheap なので最初に短絡しておく。
      if (hits === 0 && hay.indexOf(needle) === -1) continue;
    }
  }
  return results;
}

// 1 行内に複数ヒットがあっても 1 件だけ扱う簡易版なので、
// 「ヒット箇所をハイライト」する側で同じ needle を再走査して mark を付ける。
// この関数はその位置探索を返す。
export function highlightPositions(line, query, caseSensitive = false) {
  if (!query) return [];
  const hay = caseSensitive ? line : line.toLowerCase();
  const needle = caseSensitive ? query : query.toLowerCase();
  const out = [];
  let from = 0;
  while (true) {
    const i = hay.indexOf(needle, from);
    if (i === -1) break;
    out.push([i, i + query.length]);
    from = i + Math.max(1, query.length);
  }
  return out;
}
