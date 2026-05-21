import { runSearch, describeInclude } from '../js/parsed_search.js';
import { buildRows } from '../js/ci_includes.js';
import { matches as ciMatches } from '../js/views/ci_includes.js';
import { suite, test, assertEq } from './runner.js';

suite('describeInclude');

test('local', () => {
  assertEq(describeInclude({ type: 'local', file: 'ci/a.yml' }), 'ci/a.yml');
});
test('template', () => {
  assertEq(describeInclude({ type: 'template', name: 'Jobs/SAST.gitlab-ci.yml' }), 'Jobs/SAST.gitlab-ci.yml');
});
test('project with ref + files', () => {
  assertEq(describeInclude({ type: 'project', project: 'group/t', ref: 'main', files: ['a.yml', 'b.yml'] }),
    'group/t@main: a.yml, b.yml');
});
test('project without files', () => {
  assertEq(describeInclude({ type: 'project', project: 'group/t', ref: 'main', files: [] }),
    'group/t@main');
});
test('unknown type → empty', () => {
  assertEq(describeInclude({ type: 'mystery' }), '');
});

// ---------- runSearch ----------

const repo1 = { id: 1, path: 'g/a' };
const repo2 = { id: 2, path: 'g/b' };

suite('parsed_search: dep');

const depItems = [
  { repo: repo1, file: { path: 'package.json', parsed: {
      dependencies:    { lodash: '^4', react: '^18' },
      devDependencies: { jest: '^29' },
      peerDependencies:{},
  } } },
  { repo: repo2, file: { path: 'fe/package.json', parsed: {
      dependencies: { 'react-dom': '^18' },
  } } },
];

test('finds in dependencies across repos', () => {
  const r = runSearch('dep', 'react', depItems);
  assertEq(r.length, 2);
  assertEq(r[0].name, 'react');
  assertEq(r[0].scope, 'dependencies');
});

test('finds in devDependencies', () => {
  const r = runSearch('dep', 'jest', depItems);
  assertEq(r.length, 1);
  assertEq(r[0].scope, 'devDependencies');
});

test('case insensitive', () => {
  assertEq(runSearch('dep', 'LODASH', depItems).length, 1);
});

test('empty query → []', () => {
  assertEq(runSearch('dep', '', depItems), []);
});

suite('parsed_search: mvn');

const mvnItems = [
  { repo: repo1, file: { path: 'pom.xml', parsed: {
      dependencies: [
        { groupId: 'org.junit.jupiter', artifactId: 'junit-jupiter', version: '5.10.0', scope: 'test' },
        { groupId: 'org.springframework', artifactId: 'spring-core', version: '6.0.0' },
      ],
  } } },
];

test('matches groupId:artifactId substring', () => {
  const r = runSearch('mvn', 'junit', mvnItems);
  assertEq(r.length, 1);
  assertEq(r[0].artifactId, 'junit-jupiter');
});

test('matches groupId prefix', () => {
  const r = runSearch('mvn', 'springframework', mvnItems);
  assertEq(r.length, 1);
});

suite('parsed_search: image');

const imageItems = [
  { repo: repo1, file: { path: '.gitlab-ci.yml', parsed: { images: ['node:18', 'alpine:3'] } } },
  { repo: repo2, file: { path: '.gitlab-ci.yml', parsed: { images: ['node:20'] } } },
];

test('finds node images', () => {
  const r = runSearch('image', 'node', imageItems);
  assertEq(r.length, 2);
  assertEq(r[0].image, 'node:18');
  assertEq(r[1].image, 'node:20');
});

suite('parsed_search: include');

const incItems = [
  { repo: repo1, file: { path: '.gitlab-ci.yml', parsed: { includes: [
      { type: 'local', file: 'ci/build.yml' },
      { type: 'template', name: 'Jobs/SAST.gitlab-ci.yml' },
  ] } } },
  { repo: repo2, file: { path: '.gitlab-ci.yml', parsed: { includes: [
      { type: 'project', project: 'group/t', ref: 'main', files: ['java.yml'] },
  ] } } },
];

test('finds template by name', () => {
  const r = runSearch('include', 'SAST', incItems);
  assertEq(r.length, 1);
  assertEq(r[0].type, 'template');
});

test('finds project include', () => {
  const r = runSearch('include', 'group/t', incItems);
  assertEq(r.length, 1);
  assertEq(r[0].type, 'project');
});

suite('ci_includes.buildRows');

test('flattens all includes', () => {
  const rows = buildRows(incItems);
  assertEq(rows.length, 3);
  assertEq(rows[0].type, 'local');
  assertEq(rows[1].type, 'template');
  assertEq(rows[2].type, 'project');
});

suite('ci_includes filter: matches');

const row = { repo_path: 'g/a', path: '.gitlab-ci.yml', type: 'template', ref: 'Jobs/SAST.gitlab-ci.yml' };

test('empty query + all type → show', () => {
  assertEq(ciMatches(row, '', 'all'), true);
});
test('type filter excludes mismatching rows', () => {
  assertEq(ciMatches(row, '', 'local'), false);
  assertEq(ciMatches(row, '', 'template'), true);
});
test('query searches across columns', () => {
  assertEq(ciMatches(row, 'SAST', 'all'), true);
  assertEq(ciMatches(row, 'g/a', 'all'), true);
  assertEq(ciMatches(row, 'template', 'all'), true);
  assertEq(ciMatches(row, 'nope', 'all'), false);
});
test('query is case insensitive', () => {
  assertEq(ciMatches(row, 'sast', 'all'), true);
});
test('query + type combined (AND)', () => {
  assertEq(ciMatches(row, 'SAST', 'template'), true);
  assertEq(ciMatches(row, 'SAST', 'local'), false);
});
