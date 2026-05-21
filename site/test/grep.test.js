import { grep, highlightPositions } from '../js/grep.js';
import { suite, test, assertEq, assertTrue } from './runner.js';

suite('grep');

const ci = {
  kind: 'ci',
  entries: [
    { repo_id: 1, path: '.gitlab-ci.yml', raw: "image: node:18\nstages:\n  - build\n" },
    { repo_id: 2, path: '.gitlab-ci.yml', raw: "image: alpine\nscript:\n  - echo node\n" },
  ],
};

test('returns empty when query is empty', () => {
  assertEq(grep([ci], ''), []);
});

test('case insensitive by default', () => {
  const hits = grep([ci], 'NODE');
  assertEq(hits.length, 2);
});

test('case sensitive when requested', () => {
  const hits = grep([ci], 'NODE', { caseSensitive: true });
  assertEq(hits.length, 0);
});

test('records line_no, line, kind', () => {
  const hits = grep([ci], 'node:18');
  assertEq(hits.length, 1);
  assertEq(hits[0].repo_id, 1);
  assertEq(hits[0].line_no, 1);
  assertEq(hits[0].kind, 'ci');
  assertTrue(hits[0].line.includes('node:18'));
});

test('multiple matches across files & sources', () => {
  const pkg = {
    kind: 'pkg',
    entries: [
      { repo_id: 3, path: 'package.json', raw: '"node": ">=18"' },
    ],
  };
  const hits = grep([ci, pkg], 'node');
  // ci entry1: line1, ci entry2: line3, pkg: line1 → 3 hits
  assertEq(hits.length, 3);
});

test('maxPerFile limits hits per entry', () => {
  const many = {
    kind: 'ci',
    entries: [{ repo_id: 1, path: 'x', raw: 'a\na\na\na\na\n' }],
  };
  const hits = grep([many], 'a', { maxPerFile: 3 });
  assertEq(hits.length, 3);
});

suite('highlightPositions');

test('finds all positions', () => {
  assertEq(highlightPositions('abc abc abc', 'abc'), [[0,3], [4,7], [8,11]]);
});

test('case insensitive', () => {
  assertEq(highlightPositions('ABC abc', 'abc'), [[0,3], [4,7]]);
});

test('empty query returns []', () => {
  assertEq(highlightPositions('abc', ''), []);
});
